//! DSH adapter for the Grok File Search controller boundary.
//!
//! The upstream controller owns fuzzy matching and filesystem access. DSH
//! cannot import that runtime: the host owns search results, revision and
//! authorization. This controller keeps the same observable state-machine
//! rules (revision fence, stable selection, pending/available states) while
//! consuming only the typed host snapshot.

use crate::host_adapter::{FeatureStatus, FileSearchSnapshot};

#[derive(Debug, Default)]
pub struct FileSearchController {
    revision: u64,
    selected_id: Option<String>,
    snapshot: Option<FileSearchSnapshot>,
}

impl FileSearchController {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub fn reset(&mut self) {
        self.revision = 0;
        self.selected_id = None;
        self.snapshot = None;
    }

    /// Start one host query and invalidate the old result before the effect is
    /// submitted. A pending snapshot is visible immediately, never as fake
    /// empty success.
    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.selected_id = None;
        self.snapshot = Some(FileSearchSnapshot {
            status: FeatureStatus::Pending,
            query: query.to_string(),
            revision: self.revision,
            preview_status: FeatureStatus::Pending,
            selected_id: None,
            rows: Vec::new(),
            diagnostic: None,
        });
        self.revision
    }

    /// Install an authoritative result only when it belongs to the current
    /// query generation. Equal revisions are accepted; older and future
    /// revisions are never allowed to rewrite the active overlay.
    pub fn apply_result(&mut self, result: FileSearchSnapshot) -> bool {
        if result.revision != self.revision {
            return false;
        }
        self.selected_id = self
            .selected_id
            .clone()
            .or_else(|| result.selected_id.clone())
            .filter(|id| result.rows.iter().any(|row| row.id == *id));
        self.snapshot = Some(result);
        true
    }

    pub fn snapshot(&self) -> Option<&FileSearchSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn select(&mut self, id: Option<String>) {
        let valid = self.snapshot.as_ref().is_some_and(|snapshot| {
            id.as_deref()
                .is_some_and(|id| snapshot.rows.iter().any(|row| row.id == id))
        });
        self.selected_id = valid.then_some(id).flatten();
    }

    pub fn move_selection(&mut self, delta: isize) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if snapshot.rows.is_empty() {
            self.selected_id = None;
            return;
        }
        let current = self
            .selected_id
            .as_deref()
            .and_then(|id| snapshot.rows.iter().position(|row| row.id == id))
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta as usize)
                .min(snapshot.rows.len().saturating_sub(1))
        };
        self.selected_id = snapshot.rows.get(next).map(|row| row.id.clone());
    }

    pub fn selected_row(&self) -> Option<&crate::host_adapter::FileSearchRow> {
        let snapshot = self.snapshot.as_ref()?;
        let id = self.selected_id.as_deref()?;
        snapshot.rows.iter().find(|row| row.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_adapter::{FileSearchPreview, FileSearchRow};

    fn result(revision: u64, ids: &[&str]) -> FileSearchSnapshot {
        FileSearchSnapshot {
            status: FeatureStatus::Available,
            query: "src".into(),
            revision,
            preview_status: FeatureStatus::Unsupported,
            selected_id: None,
            rows: ids
                .iter()
                .map(|id| FileSearchRow {
                    id: (*id).into(),
                    path: (*id).into(),
                    kind: Some("file".into()),
                    preview: Some(FileSearchPreview {
                        line: Some(1),
                        snippet: "line".into(),
                    }),
                })
                .collect(),
            diagnostic: None,
        }
    }

    #[test]
    fn revision_fence_rejects_late_file_search_results() {
        let mut controller = FileSearchController::default();
        assert_eq!(controller.begin_query("src"), 1);
        assert!(
            !controller.apply_result(result(0, &["old"])),
            "old result leaked"
        );
        assert!(controller.apply_result(result(1, &["src/lib.rs", "src/main.rs"])));
        controller.move_selection(1);
        assert_eq!(controller.selected_id(), Some("src/main.rs"));
        assert!(
            !controller.apply_result(result(2, &["future"])),
            "future result leaked"
        );
        assert_eq!(
            controller.selected_row().map(|row| row.id.as_str()),
            Some("src/main.rs")
        );
    }

    #[test]
    fn selection_uses_stable_ids_after_result_reorder() {
        let mut controller = FileSearchController::default();
        controller.begin_query("src");
        assert!(controller.apply_result(result(1, &["a", "b", "c"])));
        controller.select(Some("b".into()));
        assert!(controller.apply_result(result(1, &["c", "b", "a"])));
        assert_eq!(controller.selected_id(), Some("b"));
        assert!(controller.apply_result(result(1, &["a", "c"])));
        assert_eq!(controller.selected_id(), None);
    }
}
