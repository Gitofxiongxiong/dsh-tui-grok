//! Typed DSH tool projection into Grok's renderer-neutral tool family.

use dsh_pager::{
    DshEditDetail, DshRenderBlock, DshToolCallView, DshToolDiff, DshToolKind, DshToolResult,
    DshToolResultView,
};
use serde_json::Value;

use crate::{
    scrollback::tool::{
        EditToolCallBlock, LineRange, ListDirToolCallBlock, OtherToolCallBlock, ReadLine,
        ReadToolCallBlock, SearchFileMatch, SearchInputMeta, SearchLineMatch, SearchOutputMode,
        SearchToolCallBlock, ToolCallBlock, ToolDiff, WebFetchToolCallBlock, WebSearchSource,
        WebSearchToolCallBlock,
    },
    views::execute_tool_adapter::project_execute_tool,
};

pub fn project_tool(block: &DshRenderBlock) -> Option<ToolCallBlock> {
    let DshRenderBlock::ToolCall {
        name,
        arguments,
        edit,
        view,
        result,
        ..
    } = block
    else {
        return None;
    };
    if let Some(execute) = project_execute_tool(block) {
        return Some(ToolCallBlock::Execute(execute));
    }
    let args = serde_json::from_str::<Value>(arguments).ok();
    let result_view = result.as_deref().and_then(|result| result.view.as_ref());

    match result_view {
        Some(DshToolResultView::Read {
            path,
            offset: _,
            lines,
            total_lines,
            lang,
            ..
        }) => {
            let mut read = ReadToolCallBlock::new(path);
            read.lines = lines
                .iter()
                .map(|line| ReadLine {
                    number: line.number,
                    text: line.text.clone(),
                })
                .collect();
            read.total_lines = usize::try_from(*total_lines).ok();
            read.language.clone_from(lang);
            if let (Some(first), Some(last)) = (lines.first(), lines.last()) {
                let start = usize::try_from(first.number).unwrap_or(usize::MAX);
                let end = usize::try_from(last.number).unwrap_or(usize::MAX);
                read.line_range = Some(LineRange::new(start, end));
            }
            apply_error(&mut read.error, result.as_deref(), "Read failed");
            return Some(ToolCallBlock::Read(read));
        }
        Some(DshToolResultView::SearchMatches {
            files,
            truncated,
            total,
            ..
        }) => {
            let mut search = project_search_call(name, args.as_ref(), view.as_ref(), arguments);
            search.match_count = usize::try_from(*total).unwrap_or(usize::MAX);
            search.truncated = *truncated;
            search.file_matches = files
                .iter()
                .map(|file| SearchFileMatch {
                    path: file.path.clone(),
                    matches: file
                        .matches
                        .iter()
                        .map(|matched| SearchLineMatch {
                            line_number: usize::try_from(matched.line_number).unwrap_or(usize::MAX),
                            content: matched.line.clone(),
                        })
                        .collect(),
                })
                .collect();
            apply_error(&mut search.error, result.as_deref(), "Search failed");
            return Some(ToolCallBlock::Search(search));
        }
        Some(DshToolResultView::SearchPaths {
            paths,
            truncated,
            total,
            ..
        }) => {
            let mut search = project_search_call(name, args.as_ref(), view.as_ref(), arguments);
            search.meta.output_mode = SearchOutputMode::FilesWithMatches;
            search.match_count = usize::try_from(*total).unwrap_or(usize::MAX);
            search.file_paths.clone_from(paths);
            search.truncated = *truncated;
            apply_error(&mut search.error, result.as_deref(), "Search failed");
            return Some(ToolCallBlock::Search(search));
        }
        Some(DshToolResultView::Diff { title, diffs }) => {
            let inferred_path = argument_string(args.as_ref(), &["path", "file_path"])
                .or_else(|| (diffs.len() == 1).then(|| diffs[0].path.as_str()));
            let inferred_title = inferred_path.map(|path| format!("Edit {path}"));
            return Some(ToolCallBlock::Edit(project_edit(
                title
                    .as_deref()
                    .or_else(|| view.as_ref().map(DshToolCallView::title))
                    .or(inferred_title.as_deref())
                    .unwrap_or(name),
                diffs,
                result.as_deref(),
            )));
        }
        Some(DshToolResultView::WebSearch {
            title,
            sources,
            answer,
            truncated,
        }) => {
            let mut web = WebSearchToolCallBlock::new(
                web_search_query(args.as_ref())
                    .or_else(|| title.clone())
                    .unwrap_or_else(|| name.to_string()),
            );
            web.sources = sources
                .iter()
                .map(|source| WebSearchSource {
                    url: source.url.clone(),
                    title: source.title.clone(),
                    snippet: source.snippet.clone(),
                })
                .collect();
            web.citations = sources.iter().map(|source| source.url.clone()).collect();
            web.answer.clone_from(answer);
            web.truncated = *truncated;
            apply_error(&mut web.error, result.as_deref(), "Web search failed");
            return Some(ToolCallBlock::WebSearch(web));
        }
        Some(DshToolResultView::WebFetch {
            url,
            status_code,
            truncated,
            ..
        }) => {
            let mut fetch = WebFetchToolCallBlock::new(url);
            fetch.status_code = Some(*status_code);
            fetch.truncated = *truncated;
            fetch.content = result.as_deref().and_then(result_text);
            apply_error(&mut fetch.error, result.as_deref(), "Web fetch failed");
            return Some(ToolCallBlock::WebFetch(fetch));
        }
        _ => {}
    }

    let kind = view.as_ref().map(DshToolCallView::kind);
    if matches!(
        kind,
        Some(DshToolKind::Edit | DshToolKind::Delete | DshToolKind::Move)
    ) || edit.is_some()
        || matches!(view, Some(DshToolCallView::Diff { .. }))
    {
        let diffs = tool_diffs(view.as_ref(), result.as_deref(), edit.as_ref());
        let inferred_path = argument_string(args.as_ref(), &["path", "file_path"])
            .or_else(|| (diffs.len() == 1).then(|| diffs[0].path.as_str()));
        let inferred_title = inferred_path.map(|path| format!("Edit {path}"));
        return Some(ToolCallBlock::Edit(project_edit(
            view.as_ref()
                .map(DshToolCallView::title)
                .or(inferred_title.as_deref())
                .unwrap_or(name),
            &diffs,
            result.as_deref(),
        )));
    }
    if name == "web_search" {
        let query = web_search_query(args.as_ref())
            .or_else(|| view.as_ref().map(|view| view.title().to_string()))
            .unwrap_or_else(|| name.to_string());
        let mut web = WebSearchToolCallBlock::new(query);
        apply_error(&mut web.error, result.as_deref(), "Web search failed");
        return Some(ToolCallBlock::WebSearch(web));
    }
    if matches!(kind, Some(DshToolKind::Read)) || name == "read" {
        let path = argument_string(args.as_ref(), &["path", "file_path"])
            .or_else(|| first_location(view.as_ref()))
            .or_else(|| {
                view.as_ref()
                    .map(DshToolCallView::title)
                    .and_then(|title| title.strip_prefix("Read "))
            })
            .unwrap_or(arguments);
        let mut read = ReadToolCallBlock::new(path);
        apply_error(&mut read.error, result.as_deref(), "Read failed");
        return Some(ToolCallBlock::Read(read));
    }
    if matches!(kind, Some(DshToolKind::Search))
        || matches!(name.as_str(), "grep" | "glob" | "search")
    {
        let mut search = project_search_call(name, args.as_ref(), view.as_ref(), arguments);
        apply_error(&mut search.error, result.as_deref(), "Search failed");
        return Some(ToolCallBlock::Search(search));
    }
    if matches!(name.as_str(), "list" | "list_dir" | "ls") {
        let path = argument_string(args.as_ref(), &["path", "directory"]).unwrap_or(".");
        let mut list = ListDirToolCallBlock::new(path);
        list.entries = result
            .as_deref()
            .and_then(result_text)
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        apply_error(&mut list.error, result.as_deref(), "List failed");
        return Some(ToolCallBlock::ListDir(list));
    }
    if matches!(kind, Some(DshToolKind::Fetch)) {
        let url = argument_string(args.as_ref(), &["url", "uri"]).unwrap_or(arguments);
        let mut fetch = WebFetchToolCallBlock::new(url);
        fetch.content = result.as_deref().and_then(result_text);
        apply_error(&mut fetch.error, result.as_deref(), "Fetch failed");
        return Some(ToolCallBlock::WebFetch(fetch));
    }

    let title = result_view
        .and_then(DshToolResultView::title)
        .or_else(|| view.as_ref().map(DshToolCallView::title))
        .unwrap_or(name);
    let mut other = OtherToolCallBlock::new(name, title);
    other.input = (!arguments.trim().is_empty()).then(|| arguments.clone());
    other.output_text = result.as_deref().and_then(result_text);
    apply_error(&mut other.error, result.as_deref(), "Tool failed");
    Some(ToolCallBlock::Other(other))
}

fn project_edit(
    title: &str,
    diffs: &[DshToolDiff],
    result: Option<&DshToolResult>,
) -> EditToolCallBlock {
    let mut edit = EditToolCallBlock::new(title);
    edit.diffs = diffs
        .iter()
        .map(|diff| ToolDiff {
            path: diff.path.clone(),
            old_text: diff.old_text.clone(),
            new_text: diff.new_text.clone(),
        })
        .collect();
    apply_error(&mut edit.error, result, "Edit failed");
    edit
}

fn tool_diffs<'a>(
    view: Option<&'a DshToolCallView>,
    result: Option<&'a DshToolResult>,
    edit: Option<&DshEditDetail>,
) -> Vec<DshToolDiff> {
    if let Some(DshToolResultView::Diff { diffs, .. }) = result.and_then(|r| r.view.as_ref()) {
        return diffs.clone();
    }
    if let Some(DshToolCallView::Diff { diffs, .. }) = view {
        return diffs.clone();
    }
    edit.map_or_else(Vec::new, |edit| {
        vec![DshToolDiff {
            path: edit.path.clone().unwrap_or_default(),
            old_text: Some(edit.old_text.clone()),
            new_text: edit.new_text.clone(),
        }]
    })
}

fn first_location(view: Option<&DshToolCallView>) -> Option<&str> {
    match view? {
        DshToolCallView::Generic { locations, .. } | DshToolCallView::Diff { locations, .. } => {
            locations.first().map(|location| location.path.as_str())
        }
        DshToolCallView::Terminal { .. } => None,
    }
}

fn argument_string<'a>(value: Option<&'a Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value?
            .get(*key)?
            .as_str()
            .filter(|value| !value.trim().is_empty())
    })
}

fn search_title_operand<'a>(view: Option<&'a DshToolCallView>, arguments: &'a str) -> &'a str {
    view.map(DshToolCallView::title)
        .and_then(|title| {
            title
                .strip_prefix("Search ")
                .or_else(|| title.strip_prefix("Grep "))
                .or_else(|| title.strip_prefix("Glob "))
        })
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(arguments)
}

fn project_search_call(
    name: &str,
    args: Option<&Value>,
    view: Option<&DshToolCallView>,
    arguments: &str,
) -> SearchToolCallBlock {
    let fallback = search_title_operand(view, arguments);
    let path = argument_string(args, &["path"]).map(str::to_owned);
    if name == "glob" {
        let glob = argument_string(args, &["pattern", "glob"]).unwrap_or(fallback);
        let mut search = SearchToolCallBlock::new(".");
        search.meta = SearchInputMeta {
            path,
            glob: (!glob.trim().is_empty()).then(|| glob.to_owned()),
            output_mode: SearchOutputMode::FilesWithMatches,
            ..SearchInputMeta::default()
        };
        if search.meta.glob.is_none() {
            search.pattern = fallback.to_owned();
        }
        return search;
    }
    let mut search =
        SearchToolCallBlock::new(argument_string(args, &["pattern", "query"]).unwrap_or(fallback));
    search.meta = SearchInputMeta {
        path,
        glob: argument_string(args, &["include"]).map(str::to_owned),
        output_mode: SearchOutputMode::Content,
        ..SearchInputMeta::default()
    };
    search
}

fn web_search_query(args: Option<&Value>) -> Option<String> {
    if let Some(query) = argument_string(args, &["query", "q"]) {
        return Some(query.to_owned());
    }
    let queries = args?.get("queries")?.as_array()?;
    let joined = queries
        .iter()
        .filter_map(Value::as_str)
        .filter(|query| !query.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

fn result_text(result: &DshToolResult) -> Option<String> {
    let text = result
        .blocks
        .iter()
        .map(DshRenderBlock::display_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn apply_error(target: &mut Option<String>, result: Option<&DshToolResult>, fallback: &str) {
    if result.is_some_and(|result| result.is_error) {
        *target = result
            .and_then(result_text)
            .or_else(|| Some(fallback.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::{DshReadLine, DshToolResult};

    #[test]
    fn typed_read_result_maps_without_flattening_lines() {
        let block = DshRenderBlock::ToolCall {
            name: "read".into(),
            call_id: Some("call-1".into()),
            arguments: r#"{"path":"src/lib.rs"}"#.into(),
            edit: None,
            view: None,
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::Read {
                    title: None,
                    path: "src/lib.rs".into(),
                    offset: 4,
                    lines: vec![DshReadLine {
                        number: 5,
                        text: "fn main() {}".into(),
                    }],
                    total_lines: 20,
                    lang: Some("rust".into()),
                    content: Vec::new(),
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        };
        let ToolCallBlock::Read(read) = project_tool(&block).expect("read") else {
            panic!("wrong block")
        };
        assert_eq!(read.path, "src/lib.rs");
        assert_eq!(read.line_range, Some(LineRange::new(5, 5)));
        assert_eq!(read.lines[0].number, 5);
    }

    #[test]
    fn execute_and_edit_remain_action_variants_and_do_not_join_eager_runs() {
        let execute = DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: None,
            arguments: r#"{"command":"pwd"}"#.into(),
            edit: None,
            view: None,
            result: None,
        };
        let projected = project_tool(&execute).expect("execute");
        assert!(matches!(projected, ToolCallBlock::Execute(_)));
        assert_eq!(projected.verb_group_kind(), None);
    }

    #[test]
    fn result_diff_without_title_keeps_the_call_view_title() {
        let diff = DshToolDiff {
            path: "src/mock.rs".into(),
            old_text: Some("old".into()),
            new_text: "new".into(),
        };
        let mut block = DshRenderBlock::ToolCall {
            name: "edit".into(),
            call_id: Some("call-edit".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Diff {
                title: "Edit src/mock.rs".into(),
                diffs: vec![diff.clone()],
                locations: Vec::new(),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::Diff {
                    title: None,
                    diffs: vec![diff],
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        };

        let ToolCallBlock::Edit(edit) = project_tool(&block).expect("edit") else {
            panic!("wrong block")
        };
        assert_eq!(edit.title, "Edit src/mock.rs");

        let DshRenderBlock::ToolCall { view, .. } = &mut block else {
            unreachable!()
        };
        *view = None;
        let ToolCallBlock::Edit(edit) = project_tool(&block).expect("edit") else {
            panic!("wrong block")
        };
        assert_eq!(edit.title, "Edit src/mock.rs");
    }

    #[test]
    fn presenter_edit_detail_survives_when_runtime_views_are_absent() {
        let block = DshRenderBlock::ToolCall {
            name: "edit".into(),
            call_id: Some("call-edit".into()),
            arguments: r#"{"path":"src/mock.rs"}"#.into(),
            edit: Some(DshEditDetail {
                path: Some("src/mock.rs".into()),
                old_text: "old line".into(),
                new_text: "new line".into(),
            }),
            view: None,
            result: Some(Box::new(DshToolResult {
                view: None,
                blocks: Vec::new(),
                is_error: false,
            })),
        };

        let ToolCallBlock::Edit(edit) = project_tool(&block).expect("edit") else {
            panic!("wrong block")
        };
        assert_eq!(edit.title, "Edit src/mock.rs");
        assert_eq!(edit.diffs.len(), 1);
        assert_eq!(edit.diffs[0].old_text.as_deref(), Some("old line"));
        assert_eq!(edit.diffs[0].new_text, "new line");
    }

    #[test]
    fn grep_result_keeps_path_include_and_file_groups() {
        let block = DshRenderBlock::ToolCall {
            name: "grep".into(),
            call_id: Some("g1".into()),
            arguments: r#"{"pattern":"TODO","path":"src","include":"*.rs"}"#.into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "Grep TODO in src (*.rs)".into(),
                kind: DshToolKind::Search,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::SearchMatches {
                    title: None,
                    files: vec![dsh_pager::DshSearchFile {
                        path: "src/lib.rs".into(),
                        matches: vec![dsh_pager::DshSearchMatch {
                            line_number: 9,
                            line: "// TODO".into(),
                        }],
                    }],
                    truncated: true,
                    total: 3,
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        };
        let ToolCallBlock::Search(search) = project_tool(&block).expect("search") else {
            panic!("wrong block")
        };
        assert_eq!(search.pattern, "TODO");
        assert_eq!(search.meta.path.as_deref(), Some("src"));
        assert_eq!(search.meta.glob.as_deref(), Some("*.rs"));
        assert_eq!(search.meta.output_mode, SearchOutputMode::Content);
        assert_eq!(search.match_count, 3);
        assert!(search.truncated);
        assert_eq!(search.file_matches[0].path, "src/lib.rs");
    }

    #[test]
    fn glob_without_result_is_search_not_other() {
        let block = DshRenderBlock::ToolCall {
            name: "glob".into(),
            call_id: Some("g2".into()),
            arguments: r#"{"pattern":"*.rs","path":"crates"}"#.into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "Glob *.rs in crates".into(),
                kind: DshToolKind::Search,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: None,
        };
        let ToolCallBlock::Search(search) = project_tool(&block).expect("search") else {
            panic!("wrong block")
        };
        assert_eq!(search.pattern, ".");
        assert_eq!(search.meta.glob.as_deref(), Some("*.rs"));
        assert_eq!(search.meta.path.as_deref(), Some("crates"));
        assert_eq!(search.meta.output_mode, SearchOutputMode::FilesWithMatches);
    }

    #[test]
    fn pending_web_search_is_not_classified_as_grep() {
        let block = DshRenderBlock::ToolCall {
            name: "web_search".into(),
            call_id: Some("w1".into()),
            arguments: r#"{"queries":["rust pager","scrollback"]}"#.into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "rust pager, scrollback".into(),
                kind: DshToolKind::Search,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: None,
        };
        let projected = project_tool(&block).expect("web");
        assert_eq!(
            projected.verb_group_kind(),
            Some(crate::scrollback::tool::VerbGroupKind::WebSearch)
        );
        let ToolCallBlock::WebSearch(web) = projected else {
            panic!("expected WebSearch");
        };
        assert_eq!(web.query, "rust pager, scrollback");
    }
}
