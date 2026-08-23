//! Deterministic semantic screen/reference runner for M10.
//!
//! This intentionally compares terminal-independent rows, rectangles and
//! focus ownership. ANSI bytes remain the responsibility of PTY tests.

use ratatui::layout::{Position, Rect};
use serde::{Deserialize, Serialize};

use crate::app::{AppShell, KeyOwner, Overlay};
use crate::geometry::{HitMap, HitTarget, insert_text_line};
use crate::host_adapter::GrokHostSnapshot;
use crate::views::agent::{AgentView, AgentViewLayout};

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
}

impl SemanticFrame {
    pub fn row_text(&self, role: &str) -> Option<&str> {
        self.rows
            .iter()
            .find(|row| row.role == role)
            .map(|row| row.text.as_str())
    }
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
    rows.push(SemanticRow {
        role: "header".into(),
        text: format!("DSH · GROK UI · {}", snapshot.session_title),
        rect: layout.header.into(),
    });
    if snapshot.transcript.is_empty() {
        rows.push(SemanticRow {
            role: "body-empty".into(),
            text: "No transcript events yet".into(),
            rect: layout.transcript.into(),
        });
    } else {
        let mut y = layout.transcript.y;
        for entry in &snapshot.transcript {
            for text in entry.content.display_text().split('\n') {
                if y >= layout.transcript.bottom() {
                    break;
                }
                rows.push(SemanticRow {
                    role: format!("transcript-{}", entry.kind.label().to_lowercase()),
                    text: text.to_string(),
                    rect: Rect::new(layout.transcript.x, y, layout.transcript.width, 1).into(),
                });
                y = y.saturating_add(1);
            }
        }
    }
    rows.push(SemanticRow {
        role: "prompt".into(),
        text: if snapshot.prompt.authoritative {
            "Prompt".into()
        } else {
            "Ask DeepSeek anything".into()
        },
        rect: layout.prompt.into(),
    });
    rows.push(SemanticRow {
        role: "footer".into(),
        text: "Enter send".into(),
        rect: layout.footer.into(),
    });

    let mut map = HitMap::new(area);
    let mut y = layout.transcript.y;
    for entry in &snapshot.transcript {
        for (line_index, text) in entry.content.display_text().split('\n').enumerate() {
            if y >= layout.transcript.bottom() {
                break;
            }
            insert_text_line(
                &mut map,
                HitTarget::TranscriptEntry(entry.id),
                line_index,
                layout.transcript.x,
                y,
                layout.transcript.width,
                text,
                crate::geometry::first_link_target(text),
            );
            y = y.saturating_add(1);
        }
    }
    map.insert(crate::geometry::HitRegion {
        target: HitTarget::Prompt,
        rect: layout.prompt,
        label: "prompt".into(),
        link: None,
        priority: 15,
    });
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
    }
}
