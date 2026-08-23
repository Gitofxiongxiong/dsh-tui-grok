//! Deterministic semantic screen/reference runner for M10.
//!
//! This intentionally compares terminal-independent rows, rectangles and
//! focus ownership. ANSI bytes remain the responsibility of PTY tests.

use dsh_pager::DshRenderEntry;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::app::{AppShell, KeyOwner, Overlay};
use crate::geometry::{HitMap, HitTarget, insert_text_line};
use crate::host_adapter::GrokHostSnapshot;
use crate::input::PromptViewport;
use crate::theme::Theme;
use crate::views::agent::PromptRenderState;
use crate::views::agent::{AgentView, AgentViewLayout};
use crate::views::transcript::RichTranscript;

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
        (theme.bg_base, "bg_base"),
        (theme.bg_highlight, "bg_highlight"),
        (theme.bg_hover, "bg_hover"),
        (theme.bg_light, "bg_light"),
        (theme.bg_visual, "bg_visual"),
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
        let layout = AgentView::layout(shell, area);
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
        rect: layout.header.into(),
    });
    if snapshot.transcript.is_empty() {
        rows.push(SemanticRow {
            role: "body-empty".into(),
            text: "No transcript events yet".into(),
            rect: layout.transcript.into(),
        });
    } else {
        let entries = snapshot
            .transcript
            .iter()
            .map(|row| DshRenderEntry {
                id: row.id,
                source_seq: row.source_seq,
                kind: row.kind,
                text: row.text.clone(),
                partial: false,
                lineage: Vec::new(),
                content: row.content.clone(),
            })
            .collect::<Vec<_>>();
        let rich = RichTranscript::new(
            &entries,
            layout.transcript.width.saturating_sub(1).max(1) as usize,
            *Theme::current(),
        );
        for paint in rich.visible_lines(0, layout.transcript.height) {
            if paint.screen_y >= layout.transcript.height {
                continue;
            }
            rows.push(SemanticRow {
                role: "transcript".into(),
                text: paint.line.to_string(),
                rect: Rect::new(
                    layout.transcript.x.saturating_add(1),
                    layout.transcript.y.saturating_add(paint.screen_y),
                    layout.transcript.width.saturating_sub(1),
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
            AgentView::mode_label(snapshot.prompt.default_mode),
            if snapshot.prompt.authoritative {
                "Prompt"
            } else {
                "Ask DeepSeek anything"
            }
        ),
        rect: layout.prompt.into(),
    });
    rows.push(SemanticRow {
        role: "footer".into(),
        text: "Enter send".into(),
        rect: layout.footer.into(),
    });

    let mut map = HitMap::new(area);
    let entries = snapshot
        .transcript
        .iter()
        .map(|row| DshRenderEntry {
            id: row.id,
            source_seq: row.source_seq,
            kind: row.kind,
            text: row.text.clone(),
            partial: false,
            lineage: Vec::new(),
            content: row.content.clone(),
        })
        .collect::<Vec<_>>();
    let rich = RichTranscript::new(
        &entries,
        layout.transcript.width.saturating_sub(1).max(1) as usize,
        *Theme::current(),
    );
    for paint in rich.visible_lines(0, layout.transcript.height) {
        if paint.screen_y >= layout.transcript.height {
            continue;
        }
        let text = paint.line.to_string();
        insert_text_line(
            &mut map,
            HitTarget::TranscriptEntry(paint.entry_id),
            paint.line_index,
            layout.transcript.x.saturating_add(1),
            layout.transcript.y.saturating_add(paint.screen_y),
            layout.transcript.width.saturating_sub(1),
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
    let mut prompt_buffer = Buffer::empty(area);
    let viewport = PromptViewport {
        lines: vec![String::new()],
        cursor_x: 0,
        cursor_y: 0,
    };
    AgentView::render_prompt_buffer(
        &mut prompt_buffer,
        layout.prompt,
        PromptRenderState {
            mode: snapshot.prompt.default_mode,
            running: snapshot.running,
            focused: shell.owner() == KeyOwner::Prompt,
            title: &snapshot.session_title,
            model: &snapshot.model,
            viewport: &viewport,
            empty: true,
        },
        Theme::current(),
    );
    SemanticFrame {
        width: area.width,
        height: area.height,
        rows,
        focus_owner: format_key_owner(shell.owner()),
        overlay: format_overlay(shell.overlay()),
        cursor: (shell.owner() == KeyOwner::Prompt)
            .then(|| Position::new(layout.prompt.x, layout.prompt.y).into()),
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
        cells: semantic_cells(&prompt_buffer, layout.prompt),
    }
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
        assert!(!frame.cells.is_empty());
        assert_eq!(cell_diff(&frame.cells, &frame.cells), Vec::new());
        assert_eq!(frame.cells[0].symbol, "╭");
    }
}
