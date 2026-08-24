//! Approval/question content rendered from the host-owned interaction DTO.
//!
//! The view owns only presentation state. Request identity and generation stay
//! in the runtime/effect boundary so a late response cannot answer a new modal.

use dsh_pager::{DshInteraction, DshRenderBlock};
use dsh_pager_protocol::SessionModeId;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use serde_json::{Value, json};

use crate::theme::Theme;
use crate::{
    host_adapter::TranscriptRow,
    views::{
        execute_tool_adapter::project_execute_tool,
        permission_view::{PermissionChoice, PermissionOption, PermissionViewState},
    },
};

/// Render a pending question. Approval uses the Grok-derived blocking
/// permission card and never enters this generic modal path.
pub fn render_question_content(
    buffer: &mut Buffer,
    area: Rect,
    interaction: &DshInteraction,
    selected_option: usize,
    answer_text: &str,
    pending: bool,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if !matches!(interaction, DshInteraction::Question { .. }) {
        return;
    }
    buffer.set_string(
        area.x,
        area.y,
        "Question",
        Style::default()
            .fg(theme.gray_bright)
            .add_modifier(Modifier::BOLD),
    );
    let mut y = area.y.saturating_add(2);
    match interaction {
        DshInteraction::Approval { .. } => {}
        DshInteraction::Question { questions, .. } => {
            let question = questions.first();
            let prompt = question
                .and_then(|value| value.get("question").or_else(|| value.get("text")))
                .and_then(Value::as_str)
                .unwrap_or("Please provide an answer");
            put_line(buffer, area, &mut y, prompt, theme.text_primary, theme);
            if let Some(options) = question.and_then(|value| value.get("options"))
                && let Some(options) = options.as_array()
            {
                for (index, option) in options.iter().enumerate() {
                    if y >= area.bottom().saturating_sub(1) {
                        break;
                    }
                    let label = option_label(option);
                    let marker = if index == selected_option { ">" } else { " " };
                    put_line(
                        buffer,
                        area,
                        &mut y,
                        &format!("{marker} {}. {label}", index + 1),
                        if index == selected_option {
                            theme.text_primary
                        } else {
                            theme.gray
                        },
                        theme,
                    );
                }
            }
            if y < area.bottom().saturating_sub(1) {
                put_line(
                    buffer,
                    area,
                    &mut y,
                    &format!("answer: {answer_text}"),
                    theme.text_primary,
                    theme,
                );
            }
            put_line(
                buffer,
                area,
                &mut y,
                if pending {
                    "sending response..."
                } else {
                    "1-9 choose   type text   Enter submit   Esc cancel"
                },
                theme.fuzzy_accent,
                theme,
            );
        }
    }
}

/// Project a DSH approval plus its already-streamed tool call into the
/// host-neutral state accepted by the Grok permission component.
pub fn permission_state(
    interaction: &DshInteraction,
    transcript: &[TranscriptRow],
    selected: usize,
    pending: bool,
    session_mode: SessionModeId,
) -> Option<PermissionViewState> {
    let DshInteraction::Approval {
        call_id,
        tool_name,
        reason,
        ..
    } = interaction
    else {
        return None;
    };
    let execute = call_id.as_deref().and_then(|wanted| {
        transcript
            .iter()
            .rev()
            .flat_map(|row| row.content.blocks.iter())
            .find(|block| {
                matches!(
                    block,
                    DshRenderBlock::ToolCall {
                        call_id: Some(candidate),
                        ..
                    } if candidate == wanted
                )
            })
            .and_then(project_execute_tool)
    });
    let command = execute.as_ref().map(|execute| execute.command.clone());
    let linked_title = execute
        .as_ref()
        .and_then(|execute| execute.description.clone())
        .filter(|value| !value.trim().is_empty());
    let fallback_title = tool_name.as_deref().map_or_else(
        || "Allow this action?".to_string(),
        |tool| format!("Allow {tool}?"),
    );
    let title = linked_title
        .clone()
        .or_else(|| reason.clone().filter(|value| !value.trim().is_empty()))
        .unwrap_or(fallback_title);
    let description = reason
        .iter()
        .filter(|value| !value.trim().is_empty() && value.as_str() != title)
        .cloned()
        .collect();
    let mut state = PermissionViewState {
        title,
        command,
        description,
        options: {
            let mut options = vec![PermissionOption {
                choice: PermissionChoice::AllowOnce,
                label: "Yes, proceed".into(),
            }];
            if session_mode == SessionModeId::Normal {
                options.push(PermissionOption {
                    choice: PermissionChoice::DontAskAgain,
                    label: "Yes, don't ask again this conversation".into(),
                });
            }
            options.push(PermissionOption {
                choice: PermissionChoice::Reject,
                label: "No, reject".into(),
            });
            options
        },
        active_idx: selected,
        args_expanded: false,
        pending,
    };
    state.clamp_selection();
    Some(state)
}

pub const fn approval_outcome(choice: PermissionChoice) -> &'static str {
    match choice {
        PermissionChoice::AllowOnce | PermissionChoice::DontAskAgain => "allowed-once",
        PermissionChoice::Reject => "rejected",
    }
}

/// Build the protocol response for the currently displayed interaction.
pub fn response_for(
    interaction: &DshInteraction,
    selected_option: usize,
    answer_text: &str,
) -> Option<dsh_pager_protocol::TuiInteractionResponse> {
    match interaction {
        DshInteraction::Approval { .. } => None,
        DshInteraction::Question { questions, .. } => {
            let question = questions.first();
            let id = question
                .and_then(|value| value.get("id").or_else(|| value.get("questionId")))
                .and_then(Value::as_str)
                .unwrap_or("q1");
            let selected = if !answer_text.trim().is_empty() {
                answer_text.trim().to_string()
            } else {
                question
                    .and_then(|value| value.get("options"))
                    .and_then(Value::as_array)
                    .and_then(|options| options.get(selected_option))
                    .map(option_value)
                    .unwrap_or_default()
            };
            if selected.is_empty() {
                return None;
            }
            Some(dsh_pager_protocol::TuiInteractionResponse::Question {
                answers: json!({
                    "answers": [{
                        "id": id,
                        "selected": [selected],
                        "text": answer_text,
                    }]
                }),
            })
        }
    }
}

fn put_line(
    buffer: &mut Buffer,
    area: Rect,
    y: &mut u16,
    text: &str,
    fg: ratatui::style::Color,
    theme: &Theme,
) {
    if *y >= area.bottom() {
        return;
    }
    let clipped: String = text.chars().take(area.width as usize).collect();
    buffer.set_string(
        area.x,
        *y,
        clipped,
        Style::default().fg(fg).bg(theme.bg_base),
    );
    *y = (*y).saturating_add(1);
}

fn option_label(value: &Value) -> String {
    value
        .get("label")
        .or_else(|| value.get("title"))
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| option_value(value))
}

fn option_value(value: &Value) -> String {
    value
        .get("value")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| value.as_str().map(str::to_string))
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::DshInteraction;

    #[test]
    fn question_response_keeps_question_id_and_selected_option() {
        let interaction = DshInteraction::Question {
            request_id: "rpc-1".into(),
            questions: vec![json!({
                "id": "q1",
                "text": "Continue?",
                "options": [{"id": "yes", "label": "Yes"}]
            })],
        };
        let response = response_for(&interaction, 0, "").unwrap();
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["answers"]["answers"][0]["id"], "q1");
        assert_eq!(value["answers"]["answers"][0]["selected"][0], "yes");
    }

    #[test]
    fn approval_choices_use_host_supported_outcomes() {
        assert_eq!(
            approval_outcome(PermissionChoice::AllowOnce),
            "allowed-once"
        );
        assert_eq!(
            approval_outcome(PermissionChoice::DontAskAgain),
            "allowed-once"
        );
        assert_eq!(approval_outcome(PermissionChoice::Reject), "rejected");
    }

    #[test]
    fn approval_projection_joins_the_exact_tool_call_by_call_id() {
        use dsh_pager::{
            DshRenderContent, DshRenderEntryId, DshRenderFinish, DshRenderKind,
            DshRenderVisibility, DshSeq, DshToolCallView,
        };
        let transcript = vec![TranscriptRow {
            id: DshRenderEntryId::Event { seq: 1 },
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            label: "tool".into(),
            text: String::new(),
            kind: DshRenderKind::ToolCall,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Running,
            group_key: None,
            selectable: true,
            source_seq: 1,
            seq: DshSeq::new(1),
            content: DshRenderContent {
                fallback: String::new(),
                blocks: vec![DshRenderBlock::ToolCall {
                    name: "bash".into(),
                    call_id: Some("call-1".into()),
                    arguments: "{}".into(),
                    edit: None,
                    view: Some(DshToolCallView::Terminal {
                        title: "find /work -maxdepth 3".into(),
                        description: Some("List project files".into()),
                        cwd: Some("/work".into()),
                    }),
                    result: None,
                }],
            },
        }];
        let interaction = DshInteraction::Approval {
            request_id: "rpc-1".into(),
            approval_id: "approval-1".into(),
            call_id: Some("call-1".into()),
            tool_name: Some("bash".into()),
            reason: Some("sandbox escalation".into()),
        };
        let state =
            permission_state(&interaction, &transcript, 9, false, SessionModeId::Normal).unwrap();
        assert_eq!(state.title, "List project files");
        assert_eq!(state.command.as_deref(), Some("find /work -maxdepth 3"));
        assert_eq!(state.active_idx, 2);
        assert_eq!(state.options.len(), 3);
        assert_eq!(state.options[1].choice, PermissionChoice::DontAskAgain);
        let plan =
            permission_state(&interaction, &transcript, 0, false, SessionModeId::Plan).unwrap();
        assert_eq!(plan.options.len(), 2);
        assert!(
            plan.options
                .iter()
                .all(|option| option.choice != PermissionChoice::DontAskAgain)
        );
    }
}
