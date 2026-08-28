//! Host-owned interaction DTO projected onto Grok blocking composer cards.
//!
//! Presentation state lives here. Request identity and generation stay in the
//! runtime/effect boundary so a late response cannot answer a new card.

use dsh_pager::{DshInteraction, DshRenderBlock};
use serde_json::{Value, json};

use crate::{
    host_adapter::TranscriptRow,
    views::{
        execute_tool_adapter::project_execute_tool,
        permission_view::{PermissionChoice, PermissionOption, PermissionViewState},
        question_view::{QuestionOption, QuestionViewState},
    },
};

/// Project a DSH question onto the Grok question card.
pub fn question_state(
    interaction: &DshInteraction,
    selected: usize,
    pending: bool,
    args_expanded: bool,
) -> Option<QuestionViewState> {
    let DshInteraction::Question { questions, .. } = interaction else {
        return None;
    };
    let question = questions.first()?;
    let title = question
        .get("question")
        .or_else(|| question.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("Please provide an answer")
        .to_string();
    let header = question
        .get("header")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            question
                .get("intent")
                .and_then(|intent| intent.get("kind"))
                .and_then(Value::as_str)
                .and_then(|kind| match kind {
                    "plan-review" => Some("Plan review".to_string()),
                    _ => None,
                })
        });
    let detail = question
        .get("detail")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .lines()
                .map(str::to_string)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let options = question
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| QuestionOption {
            label: option_label(option),
            description: option
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .filter(|option| !option.label.is_empty())
        .collect::<Vec<_>>();
    let mut state = QuestionViewState {
        header,
        title,
        detail,
        options,
        active_idx: selected,
        args_expanded,
        pending,
    };
    state.clamp_selection();
    Some(state)
}

/// Project a DSH approval plus its already-streamed tool call into the
/// host-neutral state accepted by the Grok permission component.
pub fn permission_state(
    interaction: &DshInteraction,
    transcript: &[TranscriptRow],
    selected: usize,
    pending: bool,
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
        options: vec![
            PermissionOption {
                choice: PermissionChoice::AllowOnce,
                label: "Yes, proceed".into(),
            },
            PermissionOption {
                choice: PermissionChoice::Reject,
                label: "No, reject".into(),
            },
        ],
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
///
/// DSH `AskUserQuestionAnswerItem.selected` is option **labels**. Typed text
/// that is not one of those labels is mouse/CSI noise or free-text; it must
/// not replace the numbered choice, and single-select cannot send `custom`
/// alongside `selected`.
pub fn response_for(
    interaction: &DshInteraction,
    selected_option: usize,
    answer_text: &str,
) -> Option<dsh_pager_protocol::TuiInteractionResponse> {
    match interaction {
        DshInteraction::Approval { .. } => None,
        DshInteraction::Question { questions, .. } => {
            let question = questions.first()?;
            let id = question
                .get("id")
                .or_else(|| question.get("questionId"))
                .and_then(Value::as_str)
                .unwrap_or("q1");
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let labels = options
                .iter()
                .map(option_label)
                .filter(|label| !label.is_empty())
                .collect::<Vec<_>>();
            let typed = answer_text.trim();
            if labels.is_empty() {
                if typed.is_empty() {
                    return None;
                }
                return Some(dsh_pager_protocol::TuiInteractionResponse::Question {
                    answers: json!({
                        "answers": [{
                            "id": id,
                            "selected": [],
                            "custom": typed
                        }]
                    }),
                });
            }
            let chosen = labels
                .iter()
                .find(|label| *label == typed)
                .cloned()
                .or_else(|| labels.get(selected_option).cloned())?;
            Some(dsh_pager_protocol::TuiInteractionResponse::Question {
                answers: json!({
                    "answers": [{
                        "id": id,
                        "selected": [chosen]
                    }]
                }),
            })
        }
    }
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
        .get("label")
        .or_else(|| value.get("value"))
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| value.as_str().map(str::to_string))
        .unwrap_or_default()
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
        assert_eq!(value["answers"]["answers"][0]["selected"][0], "Yes");
        assert!(value["answers"]["answers"][0].get("text").is_none());
        assert!(value["answers"]["answers"][0].get("custom").is_none());
    }

    #[test]
    fn plan_review_submits_approve_label_even_with_mouse_garbage() {
        let interaction = DshInteraction::Question {
            request_id: "rpc-plan".into(),
            questions: vec![json!({
                "id": "plan-review",
                "question": "Approve this plan and leave plan mode?",
                "options": [
                    { "label": "Approve", "description": "Leave plan mode" },
                    { "label": "Keep planning", "description": "Stay in plan mode" }
                ],
                "intent": { "kind": "plan-review", "approve": "Approve" }
            })],
        };
        for typed in ["", "[<:;M", "  [<:;M"] {
            let response = response_for(&interaction, 0, typed).expect(typed);
            let value = serde_json::to_value(response).unwrap();
            assert_eq!(
                value["answers"]["answers"][0]["id"], "plan-review",
                "{typed}"
            );
            assert_eq!(
                value["answers"]["answers"][0]["selected"][0], "Approve",
                "{typed}"
            );
            assert!(
                value["answers"]["answers"][0].get("custom").is_none(),
                "{typed}"
            );
        }
        let keep = response_for(&interaction, 1, "").unwrap();
        let value = serde_json::to_value(keep).unwrap();
        assert_eq!(
            value["answers"]["answers"][0]["selected"][0],
            "Keep planning"
        );
        assert!(response_for(&interaction, 9, "[<:;M").is_none());
    }

    fn plan_review_interaction() -> DshInteraction {
        DshInteraction::Question {
            request_id: "rpc-plan".into(),
            questions: vec![json!({
                "id": "plan-review",
                "question": "Approve this plan and leave plan mode?",
                "options": [
                    { "label": "Approve", "description": "Leave plan mode" },
                    { "label": "Keep planning", "description": "Stay in plan mode" }
                ],
                "detail": "# Plan\n\n- ship the HTML report",
                "intent": { "kind": "plan-review", "approve": "Approve" }
            })],
        }
    }

    #[test]
    fn question_state_projects_plan_review_onto_the_grok_card() {
        let state = question_state(&plan_review_interaction(), 9, false, false).unwrap();
        assert_eq!(state.header.as_deref(), Some("Plan review"));
        assert_eq!(state.title, "Approve this plan and leave plan mode?");
        assert!(state.detail.iter().any(|line| line.contains("# Plan")));
        assert_eq!(state.options[0].label, "Approve");
        assert_eq!(state.options[1].label, "Keep planning");
        assert_eq!(state.active_idx, 1);
        assert_eq!(state.selected_label(), Some("Keep planning"));
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
        let state = permission_state(&interaction, &transcript, 9, false).unwrap();
        assert_eq!(state.title, "List project files");
        assert_eq!(state.command.as_deref(), Some("find /work -maxdepth 3"));
        assert_eq!(state.active_idx, 1);
        assert_eq!(state.options.len(), 2);
        assert_eq!(state.options[0].choice, PermissionChoice::AllowOnce);
        assert_eq!(state.options[1].choice, PermissionChoice::Reject);
    }
}
