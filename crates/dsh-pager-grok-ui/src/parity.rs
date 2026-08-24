//! Deterministic semantic screen/reference runner for M10.
//!
//! This intentionally compares terminal-independent rows, rectangles and
//! focus ownership. ANSI bytes remain the responsibility of PTY tests.

use dsh_pager::DshRenderEntry;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

use crate::app::{AppShell, KeyOwner, Overlay};
use crate::appearance::{LayoutConfig, ScrollbarConfig};
use crate::geometry::{HitMap, HitTarget, insert_text_line};
use crate::host_adapter::GrokHostSnapshot;
use crate::input::PromptEditor;
use crate::theme::Theme;
use crate::views::agent::{AgentView, AgentViewLayout, AgentViewLayoutParams, effective_compact};
use crate::views::prompt_contract::{PromptFlagContract, PromptInfoContract, PromptStyleContract};
use crate::views::prompt_widget::GrokPromptRenderer;
use crate::views::transcript::RichTranscript;
use crate::views::turn_status::{MouseButtons, TurnStatusArgs, render_turn_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl From<Rect> for SemanticRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticPosition {
    pub x: u16,
    pub y: u16,
}

impl From<Position> for SemanticPosition {
    fn from(position: Position) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRow {
    pub role: String,
    pub text: String,
    pub rect: SemanticRect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticHit {
    pub target: String,
    pub rect: SemanticRect,
    pub label: String,
}

/// Terminal-independent signature for one rendered cell. Colors are emitted
/// as stable theme roles so a reference comparison does not depend on RGB
/// choices made by a terminal profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticCell {
    pub x: u16,
    pub y: u16,
    pub symbol: String,
    pub fg: String,
    pub bg: String,
    pub modifiers: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticCellDiff {
    pub x: u16,
    pub y: u16,
    pub expected: Option<SemanticCell>,
    pub actual: Option<SemanticCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticFrame {
    pub width: u16,
    pub height: u16,
    pub rows: Vec<SemanticRow>,
    pub focus_owner: String,
    pub overlay: String,
    pub cursor: Option<SemanticPosition>,
    pub layout_revision: u64,
    pub hit_map_revision: u64,
    pub hits: Vec<SemanticHit>,
    /// Cell-level contract for the prompt chrome and editor surface.
    pub cells: Vec<SemanticCell>,
}

impl SemanticFrame {
    pub fn row_text(&self, role: &str) -> Option<&str> {
        self.rows
            .iter()
            .find(|row| row.role == role)
            .map(|row| row.text.as_str())
    }
}

/// Extract a stable cell signature from a ratatui buffer region.
pub fn semantic_cells(buffer: &Buffer, area: Rect) -> Vec<SemanticCell> {
    let theme = Theme::current();
    let mut cells = Vec::with_capacity(area.width as usize * area.height as usize);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            cells.push(SemanticCell {
                x,
                y,
                symbol: cell.symbol().to_string(),
                fg: color_role(cell.fg, theme),
                bg: color_role(cell.bg, theme),
                modifiers: cell.modifier.bits(),
            });
        }
    }
    cells
}

/// Compare two cell signatures by screen coordinate, retaining only changed
/// cells. This is intentionally independent of terminal escape sequences.
pub fn cell_diff(reference: &[SemanticCell], actual: &[SemanticCell]) -> Vec<SemanticCellDiff> {
    use std::collections::BTreeMap;
    let mut expected = BTreeMap::new();
    let mut observed = BTreeMap::new();
    for cell in reference {
        expected.insert((cell.x, cell.y), cell);
    }
    for cell in actual {
        observed.insert((cell.x, cell.y), cell);
    }
    expected
        .keys()
        .chain(observed.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|(x, y)| {
            let left = expected.get(&(x, y)).copied();
            let right = observed.get(&(x, y)).copied();
            (left != right).then(|| SemanticCellDiff {
                x,
                y,
                expected: left.cloned(),
                actual: right.cloned(),
            })
        })
        .collect()
}

fn color_role(color: Color, theme: &Theme) -> String {
    [
        (theme.accent_user, "accent_user"),
        (theme.accent_assistant, "accent_assistant"),
        (theme.accent_thinking, "accent_thinking"),
        (theme.accent_tool, "accent_tool"),
        (theme.accent_error, "accent_error"),
        (theme.accent_success, "accent_success"),
        (theme.bg_base, "bg_base"),
        (theme.bg_dark, "bg_dark"),
        (theme.bg_highlight, "bg_highlight"),
        (theme.bg_hover, "bg_hover"),
        (theme.bg_light, "bg_light"),
        (theme.bg_visual, "bg_visual"),
        (theme.prompt_border, "prompt_border"),
        (theme.prompt_border_active, "prompt_border_active"),
        (theme.diff_delete_bg, "diff_delete_bg"),
        (theme.diff_insert_bg, "diff_insert_bg"),
        (theme.diff_delete_fg, "diff_delete_fg"),
        (theme.diff_insert_fg, "diff_insert_fg"),
        (theme.md_code, "md_code"),
        (theme.md_code_bg, "md_code_bg"),
        (theme.link_fg, "link_fg"),
        (theme.gray, "gray"),
        (theme.gray_bright, "gray_bright"),
        (theme.gray_dim, "gray_dim"),
        (theme.fuzzy_accent, "fuzzy_accent"),
        (theme.text_secondary, "text_secondary"),
        (theme.text_primary, "text_primary"),
    ]
    .into_iter()
    .find_map(|(candidate, role)| (candidate == color).then_some(role.to_string()))
    .unwrap_or_else(|| format!("{color:?}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityMatrix {
    pub sizes: Vec<TerminalSize>,
    pub states: Vec<String>,
    pub inputs: Vec<String>,
}

impl Default for ParityMatrix {
    fn default() -> Self {
        Self {
            sizes: vec![
                TerminalSize {
                    width: 40,
                    height: 12,
                },
                TerminalSize {
                    width: 60,
                    height: 20,
                },
                TerminalSize {
                    width: 80,
                    height: 24,
                },
                TerminalSize {
                    width: 100,
                    height: 30,
                },
                TerminalSize {
                    width: 120,
                    height: 40,
                },
                TerminalSize {
                    width: 160,
                    height: 50,
                },
            ],
            states: [
                "empty",
                "loading",
                "running",
                "streaming",
                "completed",
                "error",
                "reconnecting",
                "modal",
                "picker",
                "queue-edit",
                "selection",
                "dashboard-peek",
                "file-search",
                "suggestions",
                "image-preview",
                "workspace",
                "agent-tasks",
                "subagents",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            inputs: [
                "key-repeat",
                "modified-key",
                "mouse-wheel",
                "mouse-click",
                "mouse-drag",
                "bracketed-paste",
                "resize-storm",
                "ctrl-c",
                "esc",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

impl ParityMatrix {
    pub fn case_count(&self) -> usize {
        self.sizes.len() * self.states.len() * self.inputs.len()
    }

    pub fn has_size(&self, width: u16, height: u16) -> bool {
        self.sizes
            .iter()
            .any(|size| size.width == width && size.height == height)
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceRunner {
    matrix: ParityMatrix,
}

impl ReferenceRunner {
    pub fn new(matrix: ParityMatrix) -> Self {
        Self { matrix }
    }

    pub fn matrix(&self) -> &ParityMatrix {
        &self.matrix
    }

    pub fn render(
        &self,
        snapshot: &GrokHostSnapshot,
        shell: &mut AppShell,
        size: TerminalSize,
    ) -> SemanticFrame {
        let area = Rect::new(0, 0, size.width, size.height);
        let compact = effective_compact(false, area.height);
        let short = area.height <= crate::views::agent::SHORT_TERMINAL_ROWS;
        let layout = AgentView::layout(
            shell,
            AgentViewLayoutParams {
                area,
                layout_cfg: LayoutConfig::default(),
                scrollbar_cfg: ScrollbarConfig::default(),
                timeline_width: crate::views::timeline::rail_width(
                    true,
                    false,
                    area.width,
                    snapshot.transcript.len(),
                ),
                prompt_height: 3,
                tasks_height: (snapshot.tasks.len() as u16).min(8),
                catalog_height: (snapshot.agent.subagents.len() as u16).min(8),
                queue_height: (snapshot.queue.len() as u16).clamp(0, 3),
                turn_status_height: u16::from(snapshot.turn_status.visible),
                banner_height: u16::from(!snapshot.prompt.suggestions.is_empty()),
                status_line_height: u16::from(snapshot.status.is_some() && !short),
                prompt_gap: u16::from(!compact && !short),
                shortcuts_height: 1,
                compact,
                ..AgentViewLayoutParams::default()
            },
        );
        render_semantic(snapshot, shell, layout, area)
    }
}

pub fn render_semantic(
    snapshot: &GrokHostSnapshot,
    shell: &AppShell,
    layout: AgentViewLayout,
    area: Rect,
) -> SemanticFrame {
    let mut rows = Vec::new();
    let connection = format!(
        "{} · {} · q{} · {}",
        snapshot.connection,
        snapshot.model,
        snapshot.queue_revision,
        if snapshot.running { "running" } else { "idle" }
    );
    rows.push(SemanticRow {
        role: "header".into(),
        text: format!("{} · {}", snapshot.session_title, connection),
        rect: layout.status_bar.into(),
    });
    if snapshot.transcript.is_empty() {
        rows.push(SemanticRow {
            role: "body-empty".into(),
            text: "No transcript events yet".into(),
            rect: layout.scrollback.into(),
        });
    } else {
        let entries = snapshot
            .transcript
            .iter()
            .map(|row| DshRenderEntry {
                id: row.id,
                source_seq: row.source_seq,
                created_at_ms: row.created_at_ms,
                started_at_ms: row.started_at_ms,
                finished_at_ms: row.finished_at_ms,
                kind: row.kind,
                text: row.text.clone(),
                partial: false,
                visibility: row.visibility,
                finish: row.finish,
                group_key: row.group_key.clone(),
                selectable: row.selectable,
                lineage: Vec::new(),
                content: row.content.clone(),
            })
            .collect::<Vec<_>>();
        let rich = RichTranscript::new(
            &entries,
            layout.scrollback_content.width.saturating_sub(1).max(1) as usize,
            *Theme::current(),
        );
        for paint in rich.visible_lines(0, layout.scrollback.height) {
            if paint.screen_y >= layout.scrollback.height {
                continue;
            }
            rows.push(SemanticRow {
                role: "transcript".into(),
                text: paint.line.to_string(),
                rect: Rect::new(
                    layout.scrollback.x.saturating_add(1),
                    layout.scrollback.y.saturating_add(paint.screen_y),
                    layout.scrollback.width.saturating_sub(1),
                    1,
                )
                .into(),
            });
        }
    }
    rows.push(SemanticRow {
        role: "prompt".into(),
        text: format!(
            "{} · {}",
            snapshot.session_mode,
            if snapshot.prompt.authoritative {
                "Prompt"
            } else {
                "Ask DeepSeek anything"
            }
        ),
        rect: layout.prompt.into(),
    });
    for (role, rect, text) in [
        (
            "tasks",
            layout.tasks,
            format!("{} task(s)", snapshot.tasks.len()),
        ),
        (
            "catalog",
            layout.catalog,
            format!("{} subagent(s)", snapshot.agent.subagents.len()),
        ),
        (
            "queue",
            layout.queue,
            format!("{} queued", snapshot.queue.len()),
        ),
    ] {
        if rect.height > 0 {
            rows.push(SemanticRow {
                role: role.into(),
                text,
                rect: rect.into(),
            });
        }
    }
    rows.push(SemanticRow {
        role: "footer".into(),
        text: "Enter send".into(),
        rect: layout.status_line.into(),
    });

    let mut map = HitMap::new(area);
    let entries = snapshot
        .transcript
        .iter()
        .map(|row| DshRenderEntry {
            id: row.id,
            source_seq: row.source_seq,
            created_at_ms: row.created_at_ms,
            started_at_ms: row.started_at_ms,
            finished_at_ms: row.finished_at_ms,
            kind: row.kind,
            text: row.text.clone(),
            partial: false,
            visibility: row.visibility,
            finish: row.finish,
            group_key: row.group_key.clone(),
            selectable: row.selectable,
            lineage: Vec::new(),
            content: row.content.clone(),
        })
        .collect::<Vec<_>>();
    let rich = RichTranscript::new(
        &entries,
        layout.scrollback_content.width.saturating_sub(1).max(1) as usize,
        *Theme::current(),
    );
    for paint in rich.visible_lines(0, layout.scrollback.height) {
        if paint.screen_y >= layout.scrollback.height {
            continue;
        }
        let text = paint.line.to_string();
        insert_text_line(
            &mut map,
            HitTarget::TranscriptEntry(paint.entry_id),
            paint.line_index,
            layout.scrollback.x.saturating_add(1),
            layout.scrollback.y.saturating_add(paint.screen_y),
            layout.scrollback.width.saturating_sub(1),
            &text,
            crate::geometry::first_link_target(&text),
        );
    }
    map.insert(crate::geometry::HitRegion {
        target: HitTarget::Prompt,
        rect: layout.prompt,
        label: "prompt".into(),
        link: None,
        priority: 15,
    });
    for (label, rect, priority) in [
        ("scrollback", layout.scrollback_content, 1),
        ("tasks", layout.tasks, 5),
        ("catalog", layout.catalog, 5),
        ("todo", layout.todo, 5),
        ("queue", layout.queue, 5),
        (
            "timeline",
            Rect::new(
                layout.timeline_x,
                layout.scrollback.y,
                layout.timeline_width,
                layout.scrollback.height,
            ),
            2,
        ),
    ] {
        map.insert(crate::geometry::HitRegion {
            target: HitTarget::Overlay(label.into()),
            rect,
            label: label.into(),
            link: None,
            priority,
        });
    }
    let mut screen_buffer = Buffer::empty(area);
    let theme = Theme::current();
    screen_buffer.set_style(area, Style::default().bg(theme.bg_base));
    for row in &rows {
        let rect = Rect::new(row.rect.x, row.rect.y, row.rect.width, row.rect.height);
        screen_buffer.set_string(
            rect.x,
            rect.y,
            &row.text,
            semantic_row_style(&row.role, theme),
        );
    }
    if layout.turn_status.height > 0 {
        let output = render_turn_status(
            &mut screen_buffer,
            layout.turn_status,
            TurnStatusArgs {
                activity: &snapshot.turn_status.activity,
                turn_elapsed: None,
                activity_elapsed: None,
                tick: 0,
                pending_user_input: snapshot.turn_status.pending_user_input,
                buttons: Some(MouseButtons::default()),
                total_tokens: snapshot.turn_status.total_tokens,
                cancelling: false,
            },
            theme,
        );
        let text = (layout.turn_status.x..layout.turn_status.right())
            .map(|x| screen_buffer[(x, layout.turn_status.y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string();
        rows.push(SemanticRow {
            role: "turn-status".into(),
            text,
            rect: layout.turn_status.into(),
        });
        if let Some(rect) = output.cancel_button {
            map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay("turn-stop".into()),
                rect,
                label: "stop current turn".into(),
                link: None,
                priority: 30,
            });
        }
    }
    let mut prompt = PromptEditor::default();
    let prompt_style = PromptStyleContract {
        focused: shell.owner() == KeyOwner::Prompt,
        compact: effective_compact(false, area.height),
        title: Some(snapshot.session_title.clone()),
        ..PromptStyleContract::default()
    };
    let prompt_info = PromptInfoContract {
        model_name: snapshot.model.clone(),
        flags: match snapshot.session_mode {
            dsh_pager_protocol::SessionModeId::Normal => Vec::new(),
            mode => vec![PromptFlagContract {
                text: mode.as_str().into(),
                color: None,
                bold: true,
            }],
        },
        ..PromptInfoContract::default()
    };
    let prompt_result = GrokPromptRenderer::default().draw(
        &mut screen_buffer,
        layout.prompt,
        prompt.textarea_mut(),
        &prompt_style,
        Some(&prompt_info),
        Theme::current(),
    );
    SemanticFrame {
        width: area.width,
        height: area.height,
        rows,
        focus_owner: format_key_owner(shell.owner()),
        overlay: format_overlay(shell.overlay()),
        cursor: prompt_result
            .cursor_pos
            .map(|(x, y)| Position::new(x, y).into()),
        layout_revision: shell.layout_revision(),
        hit_map_revision: map.revision(),
        hits: map
            .regions()
            .iter()
            .map(|hit| SemanticHit {
                target: format!("{:?}", hit.target),
                rect: hit.rect.into(),
                label: hit.label.clone(),
            })
            .collect(),
        cells: semantic_cells(&screen_buffer, area),
    }
}

fn semantic_row_style(role: &str, theme: &Theme) -> Style {
    let color = match role {
        "header" => theme.gray_bright,
        "transcript" => theme.text_primary,
        "tasks" | "catalog" | "queue" => theme.gray,
        "footer" => theme.gray_dim,
        "body-empty" => theme.gray,
        _ => theme.text_primary,
    };
    let mut style = Style::default().fg(color).bg(theme.bg_base);
    if role == "header" {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn format_key_owner(owner: KeyOwner) -> String {
    format!("{owner:?}").to_lowercase()
}

fn format_overlay(overlay: Overlay) -> String {
    format!("{overlay:?}").to_lowercase()
}

/// A stable row signature suitable for snapshot files and Grok reference
/// comparisons. Rectangles and focus are retained; ANSI is intentionally not.
pub fn semantic_signature(frame: &SemanticFrame) -> String {
    serde_json::to_string(frame).expect("semantic frame serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_covers_required_sizes_states_and_inputs() {
        let matrix = ParityMatrix::default();
        assert_eq!(matrix.sizes.len(), 6);
        assert!(matrix.has_size(40, 12));
        assert!(matrix.has_size(160, 50));
        assert!(matrix.case_count() >= 600);
    }

    #[test]
    fn turn_status_is_semantic_at_80x24_and_survives_40x12() {
        let runner = ReferenceRunner::new(ParityMatrix::default());
        let snapshot = GrokHostSnapshot::demo();
        let mut shell = AppShell::default();
        let regular = runner.render(
            &snapshot,
            &mut shell,
            TerminalSize {
                width: 80,
                height: 24,
            },
        );
        let status = regular
            .row_text("turn-status")
            .expect("regular terminal turn status");
        assert!(status.starts_with('⠋'));
        assert!(status.contains("Thinking…"));
        assert!(
            regular
                .hits
                .iter()
                .any(|hit| hit.target.contains("turn-stop"))
        );

        let narrow = runner.render(
            &snapshot,
            &mut shell,
            TerminalSize {
                width: 40,
                height: 12,
            },
        );
        let narrow_status = narrow
            .row_text("turn-status")
            .expect("short terminal keeps the active turn status");
        assert!(narrow_status.starts_with('⠋'));
        assert!(narrow_status.ends_with("[stop]"));
    }

    #[test]
    fn empty_reference_contains_geometry_and_prompt_focus() {
        let runner = ReferenceRunner::new(ParityMatrix::default());
        let mut shell = AppShell::default();
        let frame = runner.render(
            &GrokHostSnapshot::demo(),
            &mut shell,
            TerminalSize {
                width: 80,
                height: 24,
            },
        );
        assert_eq!(
            frame.row_text("body-empty"),
            Some("No transcript events yet")
        );
        assert_eq!(frame.focus_owner, "transcript");
        assert!(
            frame
                .rows
                .iter()
                .all(|row| row.rect.x + row.rect.width <= 80)
        );
        assert_eq!(frame.cells.len(), 80 * 24);
        assert_eq!(cell_diff(&frame.cells, &frame.cells), Vec::new());
        assert!(frame.cells.iter().any(|cell| cell.symbol == "╭"));
    }

    #[test]
    fn full_frame_cells_include_header_and_prompt_coordinates() {
        let runner = ReferenceRunner::new(ParityMatrix::default());
        let mut shell = AppShell::default();
        let frame = runner.render(
            &GrokHostSnapshot::demo(),
            &mut shell,
            TerminalSize {
                width: 40,
                height: 12,
            },
        );
        assert!(frame.cells.iter().any(|cell| cell.x == 0 && cell.y == 0));
        assert!(frame.cells.iter().any(|cell| cell.symbol == "╭"));
        assert!(frame.cells.iter().any(|cell| cell.y > 0));
    }
}
