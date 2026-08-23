//! Approval/question content rendered from the host-owned interaction DTO.
//!
//! The view owns only presentation state. Request identity and generation stay
//! in the runtime/effect boundary so a late response cannot answer a new modal.

use dsh_pager::DshInteraction;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use serde_json::{Value, json};

use crate::theme::Theme;

/// Render one pending interaction. Question forms intentionally keep the first
/// question simple for the vertical slice; the wire response still retains the
/// host's answer envelope and question id.
pub fn render_interaction_content(
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
    let title = match interaction {
        DshInteraction::Approval { .. } => "Approval required",
        DshInteraction::Question { .. } => "Question",
    };
    buffer.set_string(
        area.x,
        area.y,
        title,
        Style::default()
            .fg(theme.gray_bright)
            .add_modifier(Modifier::BOLD),
    );
    let mut y = area.y.saturating_add(2);
    match interaction {
        DshInteraction::Approval {
            tool_name,
            reason,
            approval_id,
            ..
        } => {
            let tool = tool_name.as_deref().unwrap_or("tool action");
            put_line(
                buffer,
                area,
                &mut y,
                &format!("Allow {tool}?"),
                theme.text_primary,
                theme,
            );
            if let Some(reason) = reason.as_deref() {
                put_line(buffer, area, &mut y, reason, theme.gray, theme);
            }
            put_line(
                buffer,
                area,
                &mut y,
                &format!("approval: {approval_id}"),
                theme.gray_dim,
                theme,
            );
            put_line(
                buffer,
                area,
                &mut y,
                if pending {
                    "sending response..."
                } else {
                    "y allow once   n deny   Esc cancel"
                },
                theme.fuzzy_accent,
                theme,
            );
        }
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

/// Build the protocol response for the currently displayed interaction.
pub fn response_for(
    interaction: &DshInteraction,
    selected_option: usize,
    answer_text: &str,
) -> Option<dsh_pager_protocol::TuiInteractionResponse> {
    match interaction {
        DshInteraction::Approval { approval_id, .. } => {
            Some(dsh_pager_protocol::TuiInteractionResponse::Approval {
                approval_id: approval_id.clone(),
                outcome: "allowed-once".into(),
            })
        }
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
}
