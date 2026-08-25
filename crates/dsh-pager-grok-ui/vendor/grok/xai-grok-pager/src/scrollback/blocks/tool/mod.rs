//! Tool-call block sum type.
//!
//! This keeps Grok Build's tool-family boundary and verb vocabulary while
//! replacing its process/runtime `BlockContext` with a value-only renderer
//! context. DSH DTO conversion lives in `scrollback_adapter::project_tool`.

mod edit;
pub mod execute;
mod list_dir;
mod other;
mod read;
mod search;
mod subagent;
mod web_fetch;
mod web_search;

pub use edit::{EditToolCallBlock, ToolDiff};
pub use execute::{ExecuteBlockContext, ExecuteBlockLine, ExecuteToolCallBlock};
pub use list_dir::ListDirToolCallBlock;
pub use other::OtherToolCallBlock;
pub use read::{LineRange, ReadLine, ReadToolCallBlock};
pub use search::{
    SearchFileMatch, SearchInputMeta, SearchLineMatch, SearchOutputMode, SearchToolCallBlock,
};
pub use subagent::{SubagentToolCallBlock, SubagentToolKind};
pub use web_fetch::WebFetchToolCallBlock;
pub use web_search::{WebSearchSource, WebSearchToolCallBlock};

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    appearance::ScrollbackAppearance,
    scrollback::types::{AccentStyle, BlockLine, BlockOutput, DisplayMode, RenderedBlock},
    theme::Theme,
};

/// Semantic class of a verb-groupable run member. Names, tense and nouns are
/// copied from Grok Build's `VerbGroupKind` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerbGroupKind {
    File,
    Skill,
    Search,
    Dir,
    WebFetch,
    WebSearch,
    MemorySearch,
    IntegrationSearch,
    Subagent,
    Command,
    EditFile,
    McpCall,
    OtherTool,
}

impl VerbGroupKind {
    pub fn verb(self, running: bool) -> &'static str {
        let (past, present) = match self {
            Self::File | Self::Skill => ("Read", "Reading"),
            Self::Search | Self::WebSearch | Self::MemorySearch | Self::IntegrationSearch => {
                ("Searched", "Searching")
            }
            Self::Dir => ("Listed", "Listing"),
            Self::WebFetch => ("Fetched", "Fetching"),
            Self::Subagent | Self::Command | Self::OtherTool => ("Ran", "Running"),
            Self::EditFile => ("Edited", "Editing"),
            Self::McpCall => ("Called", "Calling"),
        };
        if running { present } else { past }
    }

    pub fn noun(self, count: usize) -> &'static str {
        let (one, many) = match self {
            Self::File | Self::EditFile => ("file", "files"),
            Self::Skill => ("skill", "skills"),
            Self::Search => ("pattern", "patterns"),
            Self::Dir => ("dir", "dirs"),
            Self::WebFetch | Self::WebSearch => ("website", "websites"),
            Self::MemorySearch => ("memory", "memories"),
            Self::IntegrationSearch | Self::McpCall => ("MCP tool", "MCP tools"),
            Self::Subagent => ("subagent", "subagents"),
            Self::Command => ("command", "commands"),
            Self::OtherTool => ("tool", "tools"),
        };
        if count == 1 { one } else { many }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolBlockContext<'a> {
    pub mode: DisplayMode,
    pub is_running: bool,
    /// Grok lifts collapsed tool text from muted gray to the primary
    /// foreground while the entry/block is selected.
    pub is_selected: bool,
    pub width: usize,
    pub appearance: &'a ScrollbackAppearance,
    pub theme: Theme,
}

#[derive(Debug, Clone)]
pub enum ToolCallBlock {
    Execute(ExecuteToolCallBlock),
    Read(ReadToolCallBlock),
    Edit(EditToolCallBlock),
    ListDir(ListDirToolCallBlock),
    Search(SearchToolCallBlock),
    WebFetch(WebFetchToolCallBlock),
    WebSearch(WebSearchToolCallBlock),
    Subagent(SubagentToolCallBlock),
    Other(OtherToolCallBlock),
}

impl ToolCallBlock {
    pub fn render(&self, ctx: ToolBlockContext<'_>) -> RenderedBlock {
        let (mut output, failed) = match self {
            Self::Execute(block) => render_execute(block, ctx),
            Self::Read(block) => (block.output(ctx), !block.is_success()),
            Self::Edit(block) => (block.output(ctx), !block.is_success()),
            Self::ListDir(block) => (block.output(ctx), !block.is_success()),
            Self::Search(block) => (block.output(ctx), !block.is_success()),
            Self::WebFetch(block) => (block.output(ctx), !block.is_success()),
            Self::WebSearch(block) => (block.output(ctx), !block.is_success()),
            Self::Subagent(block) => (block.output(ctx), !block.is_success()),
            Self::Other(block) => (block.output(ctx), !block.is_success()),
        };
        // Keep the upstream per-tool accent contract. Read/Search/ListDir
        // deliberately have no rail; action/preview tools opt in according to
        // their mode and Execute owns its success/running/error rail.
        let accent = match self {
            Self::Execute(_) => ctx
                .appearance
                .scrollback
                .blocks
                .execute
                .accent_enabled
                .then(|| {
                    if failed {
                        AccentStyle::static_color(ctx.theme.accent_error)
                    } else if ctx.is_running {
                        AccentStyle::animated(
                            ctx.appearance.scrollback.blocks.execute.running_accent,
                        )
                    } else {
                        AccentStyle::static_color(ctx.theme.accent_success)
                    }
                }),
            Self::Edit(_) => ctx
                .appearance
                .scrollback
                .blocks
                .edit
                .accent
                .map(AccentStyle::static_color),
            Self::Read(_) | Self::ListDir(_) | Self::Search(_) | Self::Subagent(_) => None,
            Self::WebFetch(_) | Self::WebSearch(_) | Self::Other(_)
                if ctx.mode == DisplayMode::Collapsed =>
            {
                None
            }
            Self::WebFetch(_) | Self::WebSearch(_) | Self::Other(_) => Some(if failed {
                AccentStyle::static_color(ctx.theme.accent_error)
            } else if ctx.is_running {
                AccentStyle::animated(ctx.theme.accent_running)
            } else {
                AccentStyle::static_color(ctx.theme.accent_tool)
            }),
        };
        let bullet = match self {
            Self::Execute(_) => accent,
            Self::Read(_) | Self::Edit(_) | Self::ListDir(_) | Self::Search(_) => {
                failed.then(|| AccentStyle::static_color(ctx.theme.accent_error))
            }
            Self::Subagent(block) => block.bullet_style(ctx),
            Self::WebFetch(_) | Self::WebSearch(_) | Self::Other(_) if failed => {
                Some(AccentStyle::static_color(ctx.theme.accent_error))
            }
            Self::WebFetch(_) | Self::WebSearch(_) | Self::Other(_)
                if ctx.mode == DisplayMode::Collapsed =>
            {
                None
            }
            Self::WebFetch(_) | Self::WebSearch(_) | Self::Other(_) => accent,
        };
        if let Some(header) = output.lines.first_mut()
            && let Some(marker) = ctx.appearance.scrollback.blocks.tool.bullet.char()
        {
            let bullet_color = bullet.map_or_else(
                || {
                    if ctx.mode == DisplayMode::Collapsed {
                        ctx.theme.gray
                    } else {
                        ctx.theme.gray_bright
                    }
                },
                |style| style.color,
            );
            header.content.spans.insert(
                0,
                Span::styled(format!("{marker} "), Style::default().fg(bullet_color)),
            );
        }
        RenderedBlock {
            output,
            accent,
            bullet,
            background: None,
            accent_background: false,
            vpad: false,
        }
    }

    /// Eager verb-run classification. Destructive/action blocks remain
    /// standalone, matching upstream `ToolCallBlock::verb_group_kind`.
    pub fn verb_group_kind(&self) -> Option<VerbGroupKind> {
        match self {
            Self::Read(block) => Some(if block.is_skill_read() {
                VerbGroupKind::Skill
            } else {
                VerbGroupKind::File
            }),
            Self::Search(_) => Some(VerbGroupKind::Search),
            Self::ListDir(_) => Some(VerbGroupKind::Dir),
            Self::WebFetch(_) => Some(VerbGroupKind::WebFetch),
            Self::WebSearch(_) => Some(VerbGroupKind::WebSearch),
            Self::Execute(_) | Self::Edit(_) | Self::Other(_) | Self::Subagent(_) => None,
        }
    }

    pub fn label_kind(&self) -> VerbGroupKind {
        self.verb_group_kind().unwrap_or(match self {
            Self::Execute(_) => VerbGroupKind::Command,
            Self::Edit(_) => VerbGroupKind::EditFile,
            Self::Subagent(_) => VerbGroupKind::Subagent,
            Self::Other(_) => VerbGroupKind::OtherTool,
            _ => VerbGroupKind::OtherTool,
        })
    }

    pub fn is_failed(&self) -> bool {
        match self {
            Self::Execute(block) => block.error.is_some(),
            Self::Read(block) => !block.is_success(),
            Self::Edit(block) => !block.is_success(),
            Self::ListDir(block) => !block.is_success(),
            Self::Search(block) => !block.is_success(),
            Self::WebFetch(block) => !block.is_success(),
            Self::WebSearch(block) => !block.is_success(),
            Self::Subagent(block) => !block.is_success(),
            Self::Other(block) => !block.is_success(),
        }
    }

    pub fn distinct_sources(&self) -> &[String] {
        match self {
            Self::WebSearch(block) => &block.citations,
            _ => &[],
        }
    }
}

fn render_execute(block: &ExecuteToolCallBlock, ctx: ToolBlockContext<'_>) -> (BlockOutput, bool) {
    let mode = match ctx.mode {
        DisplayMode::Collapsed => execute::DisplayMode::Collapsed,
        DisplayMode::Truncated => execute::DisplayMode::Truncated,
        DisplayMode::Expanded => execute::DisplayMode::Expanded,
    };
    let mut execute_ctx = ExecuteBlockContext::new(mode, ctx.is_running, ctx.width, &ctx.theme);
    let config = &ctx.appearance.scrollback.blocks.execute;
    execute_ctx.muted_command_collapsed =
        config.muted_command_collapsed && !ctx.is_running && !ctx.is_selected;
    execute_ctx.first_lines = usize::from(config.first_lines);
    execute_ctx.last_lines = usize::from(config.last_lines);
    let lines = block
        .output(&execute_ctx)
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, mut line)| {
            if let Some(background) = line.panel_background {
                for span in &mut line.content.spans {
                    span.style = span.style.bg(background);
                }
            }
            BlockLine {
                content: line.content,
                background: line.panel_background,
                bg_start_col: 0,
                background_is_panel: line.panel_background.is_some(),
                selectable: true,
                header: index == 0,
                joiner: line.joiner,
            }
        })
        .collect();
    (BlockOutput { lines }, block.error.is_some())
}

pub(super) fn header_line(
    verb: &str,
    subject: &str,
    detail: Option<&str>,
    ctx: ToolBlockContext<'_>,
) -> BlockLine {
    let muted = ctx.mode == DisplayMode::Collapsed
        && ctx.appearance.scrollback.blocks.tool.muted_collapsed
        && !ctx.is_running
        && !ctx.is_selected;
    let primary = if muted {
        ctx.theme.gray
    } else {
        ctx.theme.text_primary
    };
    let mut spans = Vec::with_capacity(3);
    if !verb.is_empty() {
        spans.push(Span::styled(
            format!("{verb} "),
            Style::default().fg(primary).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        subject.to_string(),
        Style::default().fg(primary),
    ));
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        spans.push(Span::styled(
            format!(" {detail}"),
            Style::default().fg(if ctx.appearance.scrollback.blocks.tool.dim_details {
                ctx.theme.gray_dim
            } else {
                ctx.theme.gray
            }),
        ));
    }
    BlockLine::header(Line::from(spans))
}

pub(super) fn text_line(text: impl Into<String>, ctx: ToolBlockContext<'_>) -> BlockLine {
    BlockLine::content(Line::from(Span::styled(
        text.into(),
        Style::default().fg(ctx.theme.text_secondary),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::GrokAppearanceSnapshot;

    fn context(mode: DisplayMode, running: bool) -> ToolBlockContext<'static> {
        let theme = *Theme::current();
        let appearance = Box::leak(Box::new(
            GrokAppearanceSnapshot::default().scrollback(theme),
        ));
        ToolBlockContext {
            mode,
            is_running: running,
            is_selected: false,
            width: 80,
            appearance,
            theme,
        }
    }

    #[test]
    fn verb_vocabulary_matches_upstream_tense_and_plurality() {
        assert_eq!(VerbGroupKind::File.verb(false), "Read");
        assert_eq!(VerbGroupKind::Search.verb(true), "Searching");
        assert_eq!(VerbGroupKind::WebSearch.noun(1), "website");
        assert_eq!(VerbGroupKind::WebSearch.noun(2), "websites");
    }

    #[test]
    fn only_non_destructive_tools_join_eager_runs() {
        assert_eq!(
            ToolCallBlock::Read(ReadToolCallBlock::new("a.rs")).verb_group_kind(),
            Some(VerbGroupKind::File)
        );
        assert_eq!(
            ToolCallBlock::Execute(ExecuteToolCallBlock::new("pwd")).verb_group_kind(),
            None
        );
        assert_eq!(
            ToolCallBlock::Edit(EditToolCallBlock::new("Edit a.rs")).verb_group_kind(),
            None
        );
        assert_eq!(
            ToolCallBlock::Subagent(SubagentToolCallBlock::new(
                "分析 Rust 工作区架构",
                SubagentToolKind::Running
            ))
            .verb_group_kind(),
            None
        );
        assert_eq!(
            ToolCallBlock::Subagent(SubagentToolCallBlock::new(
                "分析 Rust 工作区架构",
                SubagentToolKind::Running
            ))
            .label_kind(),
            VerbGroupKind::Subagent
        );
    }

    #[test]
    fn tool_accents_follow_upstream_per_variant_contract() {
        let running_read = ToolCallBlock::Read(ReadToolCallBlock::new("a.rs"))
            .render(context(DisplayMode::Collapsed, true));
        assert_eq!(running_read.accent, None);

        let failed = ToolCallBlock::Read(ReadToolCallBlock::new("gone.rs").with_error("not found"))
            .render(context(DisplayMode::Expanded, false));
        assert_eq!(failed.accent, None);
        assert_eq!(
            failed.bullet.map(|accent| accent.color),
            Some(Theme::current().accent_error)
        );

        let execute = ToolCallBlock::Execute(ExecuteToolCallBlock::new("pwd"))
            .render(context(DisplayMode::Collapsed, true));
        assert!(execute.accent.is_some_and(|accent| accent.animated));

        let collapsed_web =
            ToolCallBlock::WebFetch(WebFetchToolCallBlock::new("https://example.test"))
                .render(context(DisplayMode::Collapsed, true));
        assert_eq!(collapsed_web.accent, None);

        let expanded_web =
            ToolCallBlock::WebFetch(WebFetchToolCallBlock::new("https://example.test"))
                .render(context(DisplayMode::Expanded, true));
        assert!(expanded_web.accent.is_some_and(|accent| accent.animated));
    }

    #[test]
    fn running_or_selected_collapsed_tool_text_is_not_muted() {
        let block = ToolCallBlock::Read(ReadToolCallBlock::new("src/lib.rs"));
        let completed = block.render(context(DisplayMode::Collapsed, false));
        assert_eq!(
            completed.output.lines[0].content.spans[1].style.fg,
            Some(Theme::current().gray)
        );

        let running = block.render(context(DisplayMode::Collapsed, true));
        assert_eq!(
            running.output.lines[0].content.spans[1].style.fg,
            Some(Theme::current().text_primary)
        );

        let mut selected_ctx = context(DisplayMode::Collapsed, false);
        selected_ctx.is_selected = true;
        let selected = block.render(selected_ctx);
        assert_eq!(
            selected.output.lines[0].content.spans[1].style.fg,
            Some(Theme::current().text_primary)
        );
    }

    #[test]
    fn dsh_presentable_tool_family_uses_one_header_and_mode_contract() {
        let mut read = ReadToolCallBlock::new("src/lib.rs");
        read.lines.push(ReadLine {
            number: 1,
            text: "pub fn run() {}".into(),
        });
        let mut list = ListDirToolCallBlock::new("src");
        list.entries = vec!["lib.rs".into(), "main.rs".into()];
        let mut search = SearchToolCallBlock::new("TODO");
        search.match_count = 1;
        search.file_paths.push("src/lib.rs".into());
        let mut fetch = WebFetchToolCallBlock::new("https://example.test");
        fetch.status_code = Some(200);
        fetch.content = Some("body".into());
        let mut web = WebSearchToolCallBlock::new("rust pager");
        web.sources.push(WebSearchSource {
            url: "https://example.test/rust".into(),
            title: Some("Rust pager".into()),
            snippet: Some("snippet".into()),
        });
        let mut other = OtherToolCallBlock::new("custom", "Custom tool");
        other.output_text = Some("result".into());

        let family = vec![
            ToolCallBlock::Read(read),
            ToolCallBlock::ListDir(list),
            ToolCallBlock::Search(search),
            ToolCallBlock::WebFetch(fetch),
            ToolCallBlock::WebSearch(web),
            ToolCallBlock::Other(other),
        ];
        for tool in family {
            let collapsed = tool.render(context(DisplayMode::Collapsed, false));
            assert_eq!(collapsed.output.lines.len(), 1);
            assert!(collapsed.output.lines[0].header);
            assert!(
                collapsed.output.lines[0]
                    .content
                    .to_string()
                    .starts_with('◆')
            );

            let expanded = tool.render(context(DisplayMode::Expanded, false));
            assert!(expanded.output.lines.len() >= collapsed.output.lines.len());
            assert!(expanded.output.lines[0].header);
        }
    }

    #[test]
    fn web_search_keeps_distinct_source_identity_for_group_labels() {
        let mut block = WebSearchToolCallBlock::new("query");
        block.citations = vec!["https://a.test/one".into(), "https://b.test/two".into()];
        let tool = ToolCallBlock::WebSearch(block);
        assert_eq!(tool.distinct_sources().len(), 2);
        assert_eq!(tool.verb_group_kind(), Some(VerbGroupKind::WebSearch));
    }
}
