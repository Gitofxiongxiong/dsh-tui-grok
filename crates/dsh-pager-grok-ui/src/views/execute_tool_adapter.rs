//! DSH-neutral projection into Grok's vendored `ExecuteToolCallBlock`.
//!
//! This is the only module that knows both the Harness presentation DTO and
//! the Grok-derived Execute component. The component itself stays host-free.

use dsh_pager::{DshRenderBlock, DshToolCallView, DshToolKind, DshToolResult, DshToolResultView};

use super::execute_tool::ExecuteToolCallBlock;

pub fn project_execute_tool(block: &DshRenderBlock) -> Option<ExecuteToolCallBlock> {
    let DshRenderBlock::ToolCall {
        name,
        arguments,
        view,
        result,
        ..
    } = block
    else {
        return None;
    };
    let arguments = serde_json::from_str::<serde_json::Value>(arguments).ok();
    let argument_command = argument_string(arguments.as_ref(), "command");
    let argument_description = argument_string(arguments.as_ref(), "description");
    match view.as_ref() {
        Some(DshToolCallView::Terminal {
            title, description, ..
        }) => {
            let mut execute = ExecuteToolCallBlock::new(title.clone());
            if let Some(description) = nonempty(description.as_deref()).or(argument_description) {
                execute = execute.with_description(description);
            }
            apply_result(execute, result.as_deref())
        }
        Some(DshToolCallView::Generic {
            title,
            kind: DshToolKind::Execute,
            raw_input,
            content,
            ..
        }) => {
            let command = raw_input
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .or(argument_command)
                .unwrap_or(title);
            let mut execute = ExecuteToolCallBlock::new(command);
            if let Some(description) = first_text(content).or(argument_description) {
                execute = execute.with_description(description);
            }
            apply_result(execute, result.as_deref())
        }
        None if matches!(name.as_str(), "bash" | "pwsh") => {
            let command = argument_command?;
            let mut execute = ExecuteToolCallBlock::new(command);
            if let Some(description) = argument_description {
                execute = execute.with_description(description);
            }
            apply_result(execute, result.as_deref())
        }
        _ => None,
    }
}

fn argument_string<'a>(arguments: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    nonempty(arguments?.get(key)?.as_str())
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn apply_result(
    mut execute: ExecuteToolCallBlock,
    result: Option<&DshToolResult>,
) -> Option<ExecuteToolCallBlock> {
    let Some(result) = result else {
        return Some(execute);
    };
    let output = match result.view.as_ref() {
        Some(DshToolResultView::Terminal { output, .. }) => output.clone(),
        Some(DshToolResultView::Generic { content, .. }) => joined_text(content),
        _ => joined_text(&result.blocks),
    };
    if let Some(output) = output.filter(|output| !output.is_empty()) {
        execute = execute.with_output(output);
    }
    if result.is_error {
        let error = match result.view.as_ref() {
            Some(DshToolResultView::Terminal {
                signal: Some(signal),
                ..
            }) => format!("signal {signal}"),
            Some(DshToolResultView::Terminal {
                exit_code: Some(code),
                ..
            }) => format!("exit {code}"),
            _ => "Command failed".to_string(),
        };
        execute = execute.with_error(error);
    }
    Some(execute)
}

fn first_text(blocks: &[DshRenderBlock]) -> Option<&str> {
    blocks.iter().find_map(|block| match block {
        DshRenderBlock::Markdown { text } | DshRenderBlock::Plain { text }
            if !text.trim().is_empty() =>
        {
            Some(text.as_str())
        }
        _ => None,
    })
}

fn joined_text(blocks: &[DshRenderBlock]) -> Option<String> {
    let text = blocks
        .iter()
        .map(DshRenderBlock::display_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::views::execute_tool::{DisplayMode, ExecuteBlockContext};

    #[test]
    fn terminal_card_maps_into_the_grok_execute_component() {
        let block = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("call-1".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Terminal {
                title: "pwd".into(),
                description: Some("Query the workspace".into()),
                cwd: Some("/work".into()),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::Terminal {
                    title: None,
                    output: Some("/work\n".into()),
                    exit_code: Some(0),
                    signal: None,
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        };
        let execute = project_execute_tool(&block).expect("execute projection");
        let theme = Theme::current();
        let collapsed = execute.output(&ExecuteBlockContext::new(
            DisplayMode::Collapsed,
            false,
            80,
            theme,
        ));
        assert_eq!(
            collapsed.lines[0].content.to_string(),
            "Run Query the workspace"
        );
        let expanded = execute.output(&ExecuteBlockContext::new(
            DisplayMode::Expanded,
            false,
            80,
            theme,
        ));
        let text = expanded
            .lines
            .iter()
            .map(|line| line.content.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("$ pwd"));
        assert!(text.contains("/work"));
    }

    #[test]
    fn background_generic_card_uses_content_as_description_not_output() {
        let block = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("call-bg".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "node worker.js".into(),
                kind: DshToolKind::Execute,
                raw_input: Some(serde_json::json!("node worker.js")),
                content: vec![DshRenderBlock::Markdown {
                    text: "Run retry jobs".into(),
                }],
                locations: Vec::new(),
            }),
            result: None,
        };
        let execute = project_execute_tool(&block).expect("background execute projection");
        assert_eq!(execute.command, "node worker.js");
        assert_eq!(execute.description.as_deref(), Some("Run retry jobs"));
        assert!(execute.output.is_none());
    }

    #[test]
    fn shell_arguments_restore_description_when_presenter_view_is_missing() {
        let block = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("call-old".into()),
            arguments:
                r#"{"command":"cargo test --workspace","description":"Verify the workspace"}"#
                    .into(),
            edit: None,
            view: None,
            result: None,
        };
        let execute = project_execute_tool(&block).expect("argument fallback");
        assert_eq!(execute.command, "cargo test --workspace");
        assert_eq!(execute.description.as_deref(), Some("Verify the workspace"));
    }

    #[test]
    fn presenter_description_wins_over_argument_fallback() {
        let block = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("call-new".into()),
            arguments: r#"{"command":"pwd","description":"Argument summary"}"#.into(),
            edit: None,
            view: Some(DshToolCallView::Terminal {
                title: "pwd".into(),
                description: Some("Presenter summary".into()),
                cwd: None,
            }),
            result: None,
        };
        let execute = project_execute_tool(&block).expect("terminal projection");
        assert_eq!(execute.description.as_deref(), Some("Presenter summary"));
    }

    #[test]
    fn blank_presenter_description_falls_back_to_arguments() {
        let block = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("call-blank".into()),
            arguments: r#"{"command":"pwd","description":"Print the workspace"}"#.into(),
            edit: None,
            view: Some(DshToolCallView::Terminal {
                title: "pwd".into(),
                description: Some("  ".into()),
                cwd: None,
            }),
            result: None,
        };
        let execute = project_execute_tool(&block).expect("terminal projection");
        assert_eq!(execute.description.as_deref(), Some("Print the workspace"));
    }
}
