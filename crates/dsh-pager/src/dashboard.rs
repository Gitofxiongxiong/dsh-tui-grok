//! M2 dashboard state derived from host-owned session summaries.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use dsh_pager_protocol::SessionSummary;

use crate::control_plane::{SessionControlSnapshot, WorkspaceView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum DashboardStatus {
    NeedsInput,
    Failed,
    Running,
    Idle,
    Blank,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardRow {
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub parent_session_id: Option<String>,
    pub origin: Option<String>,
    pub updated_at: f64,
    pub status: DashboardStatus,
    pub depth: usize,
    pub workspace_id: Option<String>,
    pub workspace_title: Option<String>,
    pub workspace_path: Option<String>,
    pub jobs: usize,
    pub running_jobs: usize,
    pub stopping_jobs: usize,
    pub failed_jobs: usize,
    pub latest_job_error: Option<String>,
    pub pending_interactions: usize,
    pub archived: bool,
    pub removed: bool,
    pub inactive: bool,
    pub has_children: bool,
}

/// Host-owned workspace metadata used by the Dashboard grouping view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardWorkspace {
    pub workspace_id: String,
    pub title: String,
    pub path: String,
    pub order: usize,
}

/// Row-level action lifecycle. Admission is distinct from the eventual host
/// state; a pending action never implies that the session already changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum DashboardActionKind {
    Reply,
    Dispatch,
    Attach,
    Cancel,
    Rename,
    Fork,
    Archive,
    Reorder,
    Peek,
    FollowUp,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardActionState {
    Available,
    Confirming,
    Pending { request_id: String, generation: u64 },
    Accepted,
    Rejected(String),
    Stale,
    Unavailable(String),
}

/// View-only state saved across Dashboard → attach/detail → back transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardViewState {
    pub query: String,
    pub selected_id: Option<String>,
    pub show_archived: bool,
    pub collapse_inactive: bool,
    pub group_by_workspace: bool,
    pub status_filter: Option<DashboardStatus>,
    pub collapsed_workspaces: BTreeSet<String>,
    pub collapsed_sessions: BTreeSet<String>,
}

#[derive(Debug)]
pub struct DashboardModel {
    rows: Vec<DashboardRow>,
    workspaces: Vec<DashboardWorkspace>,
    workspace_order: Vec<String>,
    session_order: HashMap<(String, String), usize>,
    query: String,
    selected_id: Option<String>,
    show_archived: bool,
    collapse_inactive: bool,
    group_by_workspace: bool,
    status_filter: Option<DashboardStatus>,
    collapsed_workspaces: BTreeSet<String>,
    collapsed_sessions: BTreeSet<String>,
    actions: BTreeMap<(String, DashboardActionKind), DashboardActionState>,
}

impl Default for DashboardModel {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            workspaces: Vec::new(),
            workspace_order: Vec::new(),
            session_order: HashMap::new(),
            query: String::new(),
            selected_id: None,
            show_archived: true,
            collapse_inactive: false,
            group_by_workspace: true,
            status_filter: None,
            collapsed_workspaces: BTreeSet::new(),
            collapsed_sessions: BTreeSet::new(),
            actions: BTreeMap::new(),
        }
    }
}

impl DashboardModel {
    pub fn replace(&mut self, summaries: Vec<SessionSummary>) {
        let parents = summaries
            .iter()
            .map(|summary| {
                (
                    summary.session_id.clone(),
                    summary.parent_session_id.clone(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut rows = summaries
            .into_iter()
            .map(|summary| {
                let depth = lineage_depth(summary.parent_session_id.as_deref(), &parents);
                let title = summary
                    .projections
                    .as_ref()
                    .and_then(|projection| projection.values.get("title"))
                    .and_then(|value| value.as_str())
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(&summary.session_id)
                    .to_string();
                let status = dashboard_status(&summary);
                DashboardRow {
                    session_id: summary.session_id,
                    title,
                    cwd: summary.cwd,
                    parent_session_id: summary.parent_session_id,
                    origin: summary.origin,
                    updated_at: summary.updated_at,
                    status,
                    depth,
                    workspace_id: None,
                    workspace_title: None,
                    workspace_path: None,
                    jobs: 0,
                    running_jobs: 0,
                    stopping_jobs: 0,
                    failed_jobs: 0,
                    latest_job_error: None,
                    pending_interactions: 0,
                    archived: false,
                    removed: false,
                    inactive: !summary.running && !summary.blank,
                    has_children: false,
                }
            })
            .collect::<Vec<_>>();
        // Keep roots together and show recent children within each lineage
        // level. This makes a refreshed list deterministic while preserving a
        // compact tree shape for the first dashboard slice.
        rows.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| right.updated_at.total_cmp(&left.updated_at))
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
        self.rows = rows;
        self.workspaces.clear();
        self.workspace_order.clear();
        self.session_order.clear();
        self.recompute_children();
        self.retain_selection();
    }

    /// Replace rows from the all-session control-plane snapshots while
    /// preserving the selected stable session id.
    pub fn replace_control_plane(&mut self, snapshots: Vec<SessionControlSnapshot>) {
        self.replace_control_plane_with_workspaces(snapshots, Vec::new(), Vec::new());
    }

    /// Replace rows and the independent workspace baseline. The two baselines
    /// may arrive in either order during reconnect, so the Dashboard model
    /// accepts a missing workspace list and keeps sessions ungrouped.
    pub fn replace_control_plane_with_workspaces(
        &mut self,
        snapshots: Vec<SessionControlSnapshot>,
        workspaces: Vec<WorkspaceView>,
        workspace_order: Vec<String>,
    ) {
        // `workspaceOrder` orders the groups, while each WorkspaceView's
        // `sessionIds` is the host-owned order within that group.  Keep the
        // latter in a side map instead of adding another public row field: it
        // is display metadata, not session identity, and this preserves the
        // value-backed DashboardRow API for callers that construct rows in
        // tests or integrations.
        let session_order = workspaces
            .iter()
            .flat_map(|workspace| {
                workspace
                    .session_ids
                    .iter()
                    .enumerate()
                    .map(move |(index, session_id)| {
                        ((workspace.workspace_id.clone(), session_id.clone()), index)
                    })
            })
            .collect::<HashMap<_, _>>();
        self.session_order = session_order.clone();
        let workspace_map = workspaces
            .iter()
            .map(|workspace| {
                (
                    workspace.workspace_id.clone(),
                    (workspace.title.clone(), workspace.path.clone()),
                )
            })
            .collect::<HashMap<_, _>>();
        self.workspace_order = workspace_order;
        let mut workspace_rows = workspaces
            .into_iter()
            .map(|workspace| {
                (
                    workspace.workspace_id.clone(),
                    DashboardWorkspace {
                        workspace_id: workspace.workspace_id,
                        title: workspace.title,
                        path: workspace.path,
                        order: 0,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut ordered = Vec::new();
        for workspace_id in &self.workspace_order {
            if let Some(workspace) = workspace_rows.remove(workspace_id) {
                ordered.push(workspace);
            }
        }
        ordered.extend(workspace_rows.into_values());
        for (order, workspace) in ordered.iter_mut().enumerate() {
            workspace.order = order;
        }
        self.workspaces = ordered;
        let mut rows = snapshots
            .into_iter()
            .map(|snapshot| {
                let status = control_status(&snapshot);
                let inactive = snapshot.running != Some(true)
                    && !snapshot.removed
                    && !snapshot.archived
                    && snapshot.blank != Some(true);
                let workspace = snapshot
                    .workspace_id
                    .as_deref()
                    .and_then(|id| workspace_map.get(id));
                let running_jobs = snapshot
                    .jobs
                    .iter()
                    .filter(|job| job.status.eq_ignore_ascii_case("running"))
                    .count();
                let stopping_jobs = snapshot
                    .jobs
                    .iter()
                    .filter(|job| job.status.eq_ignore_ascii_case("stopping"))
                    .count();
                let failed_jobs = snapshot
                    .jobs
                    .iter()
                    .filter(|job| job.status.eq_ignore_ascii_case("failed"))
                    .count();
                let latest_job_error = snapshot
                    .jobs
                    .iter()
                    .rev()
                    .find(|job| job.status.eq_ignore_ascii_case("failed"))
                    .and_then(|job| job.detail.clone());
                let title = snapshot
                    .projections
                    .get("title")
                    .and_then(|projection| projection.value.as_str())
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(&snapshot.session_id)
                    .to_string();
                DashboardRow {
                    session_id: snapshot.session_id,
                    title,
                    cwd: snapshot.cwd,
                    parent_session_id: snapshot.parent_session_id,
                    origin: snapshot.origin,
                    updated_at: snapshot.updated_at_ms.unwrap_or(snapshot.last_activity_ms) as f64
                        / 1000.0,
                    status,
                    depth: 0,
                    workspace_id: snapshot.workspace_id,
                    workspace_title: workspace.map(|value| value.0.clone()),
                    workspace_path: workspace.map(|value| value.1.clone()),
                    jobs: snapshot.jobs.len(),
                    running_jobs,
                    stopping_jobs,
                    failed_jobs,
                    latest_job_error,
                    pending_interactions: snapshot.pending_interactions.len(),
                    archived: snapshot.archived,
                    removed: snapshot.removed,
                    inactive,
                    has_children: false,
                }
            })
            .collect::<Vec<_>>();
        let parents = rows
            .iter()
            .map(|row| (row.session_id.clone(), row.parent_session_id.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        for row in &mut rows {
            row.depth = lineage_depth(row.parent_session_id.as_deref(), &parents);
        }
        self.rows = rows;
        self.recompute_children();
        self.resort_rows();
        self.retain_selection();
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn show_archived(&self) -> bool {
        self.show_archived
    }

    pub fn set_show_archived(&mut self, show: bool) {
        self.show_archived = show;
        self.retain_selection();
    }

    pub fn toggle_show_archived(&mut self) {
        self.set_show_archived(!self.show_archived);
    }

    pub fn collapse_inactive(&self) -> bool {
        self.collapse_inactive
    }

    pub fn set_collapse_inactive(&mut self, collapse: bool) {
        self.collapse_inactive = collapse;
        self.retain_selection();
    }

    pub fn toggle_collapse_inactive(&mut self) {
        self.set_collapse_inactive(!self.collapse_inactive);
    }

    pub fn group_by_workspace(&self) -> bool {
        self.group_by_workspace
    }

    pub fn set_group_by_workspace(&mut self, grouped: bool) {
        self.group_by_workspace = grouped;
        self.resort_rows();
        self.retain_selection();
    }

    pub fn toggle_group_by_workspace(&mut self) {
        self.set_group_by_workspace(!self.group_by_workspace);
    }

    pub fn status_filter(&self) -> Option<DashboardStatus> {
        self.status_filter
    }

    pub fn set_status_filter(&mut self, status: Option<DashboardStatus>) {
        self.status_filter = status;
        self.retain_selection();
    }

    pub fn toggle_status_filter(&mut self, status: DashboardStatus) {
        self.set_status_filter((self.status_filter != Some(status)).then_some(status));
    }

    pub fn workspaces(&self) -> &[DashboardWorkspace] {
        &self.workspaces
    }

    /// Return the host-owned session order for one workspace. Rows omitted by
    /// a query/archive filter are intentionally retained here so a reorder
    /// action never submits an anchor derived from the filtered presentation.
    pub fn session_ids_in_workspace(&self, workspace_id: &str) -> Vec<String> {
        let mut ids = self
            .rows
            .iter()
            .filter(|row| row.workspace_id.as_deref() == Some(workspace_id))
            .map(|row| row.session_id.clone())
            .collect::<Vec<_>>();
        ids.sort_by_key(|id| {
            self.session_order
                .get(&(workspace_id.to_string(), id.clone()))
                .copied()
                .unwrap_or(usize::MAX)
        });
        ids
    }

    pub fn collapsed_workspaces(&self) -> &BTreeSet<String> {
        &self.collapsed_workspaces
    }

    pub fn collapsed_sessions(&self) -> &BTreeSet<String> {
        &self.collapsed_sessions
    }

    pub fn toggle_workspace(&mut self, workspace_id: &str) {
        toggle_set(&mut self.collapsed_workspaces, workspace_id);
        self.retain_selection();
    }

    pub fn toggle_session_tree(&mut self, session_id: &str) {
        toggle_set(&mut self.collapsed_sessions, session_id);
        self.retain_selection();
    }

    pub fn view_state(&self) -> DashboardViewState {
        DashboardViewState {
            query: self.query.clone(),
            selected_id: self.selected_id.clone(),
            show_archived: self.show_archived,
            collapse_inactive: self.collapse_inactive,
            group_by_workspace: self.group_by_workspace,
            status_filter: self.status_filter,
            collapsed_workspaces: self.collapsed_workspaces.clone(),
            collapsed_sessions: self.collapsed_sessions.clone(),
        }
    }

    pub fn restore_view_state(&mut self, state: DashboardViewState) {
        self.query = state.query;
        self.selected_id = state.selected_id;
        self.show_archived = state.show_archived;
        self.collapse_inactive = state.collapse_inactive;
        self.group_by_workspace = state.group_by_workspace;
        self.status_filter = state.status_filter;
        self.collapsed_workspaces = state.collapsed_workspaces;
        self.collapsed_sessions = state.collapsed_sessions;
        self.retain_selection();
    }

    pub fn action_state(
        &self,
        session_id: &str,
        action: DashboardActionKind,
    ) -> DashboardActionState {
        self.actions
            .get(&(session_id.to_string(), action))
            .cloned()
            .unwrap_or(DashboardActionState::Available)
    }

    pub fn begin_action(&mut self, session_id: &str, action: DashboardActionKind) -> bool {
        let key = (session_id.to_string(), action);
        let allowed = matches!(
            self.actions.get(&key),
            None | Some(DashboardActionState::Available)
                | Some(DashboardActionState::Rejected(_))
                | Some(DashboardActionState::Stale)
        );
        if allowed {
            self.actions.insert(key, DashboardActionState::Confirming);
        }
        allowed
    }

    pub fn mark_action_pending(
        &mut self,
        session_id: &str,
        action: DashboardActionKind,
        request_id: String,
        generation: u64,
    ) {
        self.actions.insert(
            (session_id.to_string(), action),
            DashboardActionState::Pending {
                request_id,
                generation,
            },
        );
    }

    pub fn resolve_action(
        &mut self,
        session_id: &str,
        action: DashboardActionKind,
        generation: u64,
        accepted: bool,
        error: Option<String>,
    ) {
        self.resolve_action_for_request(session_id, action, None, generation, accepted, error);
    }

    /// Resolve only the request that currently owns the row action. A late
    /// result from an older request in the same generation is stale too; the
    /// generation alone is not enough when a user retries quickly.
    pub fn resolve_action_for_request(
        &mut self,
        session_id: &str,
        action: DashboardActionKind,
        request_id: Option<&str>,
        generation: u64,
        accepted: bool,
        error: Option<String>,
    ) {
        let key = (session_id.to_string(), action);
        let state = match self.actions.get(&key) {
            Some(DashboardActionState::Pending {
                request_id: expected_request,
                generation: expected,
            }) if *expected != generation
                || request_id.is_some_and(|request| request != expected_request) =>
            {
                DashboardActionState::Stale
            }
            _ if accepted => DashboardActionState::Accepted,
            _ => DashboardActionState::Rejected(error.unwrap_or_else(|| "action rejected".into())),
        };
        self.actions.insert(key, state);
    }

    pub fn mark_action_unavailable(
        &mut self,
        session_id: &str,
        action: DashboardActionKind,
        reason: impl Into<String>,
    ) {
        self.actions.insert(
            (session_id.to_string(), action),
            DashboardActionState::Unavailable(reason.into()),
        );
    }

    pub fn mark_generation_stale(&mut self, generation: u64) {
        for state in self.actions.values_mut() {
            if matches!(state, DashboardActionState::Pending { generation: expected, .. } if *expected != generation)
            {
                *state = DashboardActionState::Stale;
            }
        }
    }

    pub fn rows(&self) -> Vec<&DashboardRow> {
        let query = self.query.trim().to_lowercase();
        let by_id = self
            .rows
            .iter()
            .map(|row| (row.session_id.as_str(), row))
            .collect::<HashMap<_, _>>();
        self.rows
            .iter()
            .filter(|row| self.row_matches(row, &query))
            .filter(|row| !self.hidden_by_tree(row, &by_id))
            .collect()
    }

    /// Return a bounded render window. The model still computes the full
    /// filtered roster, while the widget only allocates rows for the viewport.
    pub fn rows_window(&self, offset: usize, limit: usize) -> Vec<&DashboardRow> {
        if limit == 0 {
            return Vec::new();
        }
        self.rows().into_iter().skip(offset).take(limit).collect()
    }

    fn row_matches(&self, row: &DashboardRow, query: &str) -> bool {
        if (row.removed || (!self.show_archived && row.archived))
            && self.selected_id.as_deref() != Some(row.session_id.as_str())
        {
            return false;
        }
        if self.collapse_inactive
            && row.inactive
            && self.selected_id.as_deref() != Some(row.session_id.as_str())
        {
            return false;
        }
        if self
            .status_filter
            .is_some_and(|status| row.status != status)
        {
            return false;
        }
        query.is_empty()
            || row.session_id.to_lowercase().contains(query)
            || row.title.to_lowercase().contains(query)
            || row
                .cwd
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
            || row
                .origin
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
            || row
                .workspace_id
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
            || row
                .workspace_title
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(query)
    }

    fn hidden_by_tree(&self, row: &DashboardRow, by_id: &HashMap<&str, &DashboardRow>) -> bool {
        if self.group_by_workspace
            && row
                .workspace_id
                .as_deref()
                .is_some_and(|id| self.collapsed_workspaces.contains(id))
        {
            return true;
        }
        let mut parent = row.parent_session_id.as_deref();
        let mut seen = HashSet::new();
        while let Some(id) = parent {
            if !seen.insert(id) {
                break;
            }
            if self.collapsed_sessions.contains(id) {
                return true;
            }
            parent = by_id
                .get(id)
                .and_then(|parent| parent.parent_session_id.as_deref());
        }
        false
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub fn selected_index(&self) -> usize {
        let rows = self.rows();
        self.selected_id
            .as_deref()
            .and_then(|id| rows.iter().position(|row| row.session_id == id))
            .unwrap_or(0)
    }

    pub fn select_first(&mut self) {
        self.selected_id = self.rows().first().map(|row| row.session_id.clone());
    }

    pub fn selected(&self) -> Option<&DashboardRow> {
        let rows = self.rows();
        self.selected_id
            .as_deref()
            .and_then(|id| rows.iter().copied().find(|row| row.session_id == id))
            .or_else(|| rows.first().copied())
    }

    pub fn move_selection(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            self.selected_id = None;
            return;
        }
        let current = self
            .selected_id
            .as_deref()
            .and_then(|id| rows.iter().position(|row| row.session_id == id))
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(rows.len() as isize) as usize;
        self.selected_id = Some(rows[next].session_id.clone());
    }

    pub fn status_counts(&self) -> std::collections::BTreeMap<DashboardStatus, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for row in self.rows() {
            *counts.entry(row.status).or_insert(0) += 1;
        }
        counts
    }

    pub fn select(&mut self, session_id: impl Into<String>) {
        self.selected_id = Some(session_id.into());
    }

    fn recompute_children(&mut self) {
        let parents = self
            .rows
            .iter()
            .map(|row| (row.session_id.clone(), row.parent_session_id.clone()))
            .collect::<HashMap<_, _>>();
        let child_ids = self
            .rows
            .iter()
            .filter_map(|row| row.parent_session_id.clone())
            .collect::<HashSet<_>>();
        for row in &mut self.rows {
            row.depth = lineage_depth(row.parent_session_id.as_deref(), &parents);
            row.has_children = child_ids.contains(&row.session_id);
        }
    }

    fn resort_rows(&mut self) {
        if self.group_by_workspace {
            sort_rows(&mut self.rows, &self.workspace_order, &self.session_order);
        } else {
            self.rows.sort_by(|left, right| {
                left.depth
                    .cmp(&right.depth)
                    .then_with(|| right.updated_at.total_cmp(&left.updated_at))
                    .then_with(|| left.session_id.cmp(&right.session_id))
            });
        }
    }

    fn retain_selection(&mut self) {
        let visible = self.rows();
        if self
            .selected_id
            .as_deref()
            .is_some_and(|id| !visible.iter().any(|row| row.session_id == id))
        {
            self.selected_id = visible.first().map(|row| row.session_id.clone());
        }
    }
}

fn sort_rows(
    rows: &mut [DashboardRow],
    workspace_order: &[String],
    session_order: &HashMap<(String, String), usize>,
) {
    let workspace_rank = workspace_order
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    rows.sort_by(|left, right| {
        let left_workspace = left
            .workspace_id
            .as_deref()
            .and_then(|id| workspace_rank.get(id).copied())
            .unwrap_or(usize::MAX);
        let right_workspace = right
            .workspace_id
            .as_deref()
            .and_then(|id| workspace_rank.get(id).copied())
            .unwrap_or(usize::MAX);
        let left_session = left
            .workspace_id
            .as_ref()
            .and_then(|workspace_id| {
                session_order
                    .get(&(workspace_id.clone(), left.session_id.clone()))
                    .copied()
            })
            .unwrap_or(usize::MAX);
        let right_session = right
            .workspace_id
            .as_ref()
            .and_then(|workspace_id| {
                session_order
                    .get(&(workspace_id.clone(), right.session_id.clone()))
                    .copied()
            })
            .unwrap_or(usize::MAX);
        left_workspace
            .cmp(&right_workspace)
            .then_with(|| left_session.cmp(&right_session))
            .then_with(|| left.depth.cmp(&right.depth))
            .then_with(|| right.updated_at.total_cmp(&left.updated_at))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

fn toggle_set(set: &mut BTreeSet<String>, value: &str) {
    if !set.insert(value.to_string()) {
        set.remove(value);
    }
}

fn lineage_depth(
    parent: Option<&str>,
    parents: &std::collections::HashMap<String, Option<String>>,
) -> usize {
    let mut depth = 0;
    let mut current = parent;
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            break;
        }
        depth += 1;
        current = parents.get(id).and_then(Option::as_deref);
    }
    depth
}

fn dashboard_status(summary: &SessionSummary) -> DashboardStatus {
    let pending = summary
        .projections
        .as_ref()
        .and_then(|projection| {
            projection
                .values
                .get("pendingInteraction")
                .or_else(|| projection.values.get("interaction"))
        })
        .is_some_and(|value| !value.is_null() && value != &serde_json::Value::Bool(false));
    if pending {
        DashboardStatus::NeedsInput
    } else if summary.running {
        DashboardStatus::Running
    } else if summary.blank {
        DashboardStatus::Blank
    } else {
        DashboardStatus::Idle
    }
}

fn control_status(snapshot: &SessionControlSnapshot) -> DashboardStatus {
    if !snapshot.pending_interactions.is_empty() {
        DashboardStatus::NeedsInput
    } else if snapshot.last_error.is_some() || projection_failed(snapshot) {
        DashboardStatus::Failed
    } else if snapshot.running == Some(true) {
        DashboardStatus::Running
    } else if snapshot.blank == Some(true) {
        DashboardStatus::Blank
    } else {
        DashboardStatus::Idle
    }
}

fn projection_failed(snapshot: &SessionControlSnapshot) -> bool {
    snapshot.projections.iter().any(|(key, projection)| {
        let key = key.to_ascii_lowercase();
        if !matches!(key.as_str(), "status" | "state" | "phase" | "result") {
            return false;
        }
        projection
            .value
            .as_str()
            .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "failed" | "error"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{SessionProjectionsBlock, SessionSummary};
    use serde_json::{Map, Value, json};

    fn snapshot(id: &str, at: u64) -> SessionControlSnapshot {
        SessionControlSnapshot {
            session_id: id.into(),
            generation: 1,
            last_activity_ms: at,
            ..SessionControlSnapshot::default()
        }
    }

    fn summary(
        id: &str,
        updated_at: f64,
        running: bool,
        blank: bool,
        parent: Option<&str>,
        title: Option<&str>,
    ) -> SessionSummary {
        let mut values = Map::new();
        if let Some(title) = title {
            values.insert("title".into(), json!(title));
        }
        SessionSummary {
            session_id: id.into(),
            updated_at,
            running,
            blank,
            parent_session_id: parent.map(str::to_string),
            origin: None,
            cwd: Some("/work".into()),
            agent_preset: None,
            projections: (!values.is_empty()).then_some(SessionProjectionsBlock {
                as_of_seq: 1,
                values,
            }),
        }
    }

    #[test]
    fn dashboard_derives_title_status_and_lineage_depth() {
        let mut dashboard = DashboardModel::default();
        dashboard.replace(vec![
            summary("child", 2.0, false, false, Some("root"), Some("Child")),
            summary("root", 1.0, true, false, None, None),
        ]);
        let rows = dashboard.rows();
        assert_eq!(rows[0].session_id, "root");
        assert_eq!(rows[0].status, DashboardStatus::Running);
        assert_eq!(rows[1].title, "Child");
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn dashboard_filter_keeps_selection_by_stable_id_after_refresh() {
        let mut dashboard = DashboardModel::default();
        dashboard.replace(vec![
            summary("a", 1.0, false, false, None, Some("Alpha")),
            summary("b", 2.0, false, false, None, Some("Beta")),
        ]);
        dashboard.select("b");
        dashboard.set_query("beta");
        assert_eq!(
            dashboard
                .rows()
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
        dashboard.replace(vec![
            summary("b", 3.0, false, false, None, Some("Beta")),
            summary("a", 4.0, false, false, None, Some("Alpha")),
        ]);
        assert_eq!(dashboard.selected_id(), Some("b"));
    }

    #[test]
    fn pending_interaction_projection_has_priority_over_running_status() {
        let mut row = summary("waiting", 1.0, true, false, None, None);
        row.projections = Some(SessionProjectionsBlock {
            as_of_seq: 2,
            values: Map::from_iter([("pendingInteraction".into(), json!({ "kind": "question" }))]),
        });
        let mut dashboard = DashboardModel::default();
        dashboard.replace(vec![row]);
        assert_eq!(dashboard.rows()[0].status, DashboardStatus::NeedsInput);
    }

    #[test]
    fn dashboard_selection_wraps_over_the_filtered_stable_rows() {
        let mut dashboard = DashboardModel::default();
        dashboard.replace(vec![
            summary("a", 1.0, false, false, None, Some("Alpha")),
            summary("b", 2.0, false, false, None, Some("Beta")),
        ]);
        dashboard.set_query("alpha");
        dashboard.move_selection(1);
        assert_eq!(dashboard.selected_id(), Some("a"));
        assert_eq!(
            dashboard.status_counts().get(&DashboardStatus::Idle),
            Some(&1)
        );
    }

    #[test]
    fn control_plane_rows_expose_jobs_interactions_and_failed_priority() {
        let mut store = crate::control_plane::ControlPlaneStore::default();
        store.set_generation(2);
        store
            .apply_notification(&dsh_pager_protocol::JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "generation": 2,
                    "type": "host/session-added",
                    "sessionId": "s",
                    "blank": false,
                    "cwd": "/work"
                })),
            })
            .unwrap();
        store
            .apply_notification(&dsh_pager_protocol::JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "generation": 2,
                    "type": "host/agent-error",
                    "sessionId": "s",
                    "message": "failed"
                })),
            })
            .unwrap();
        store
            .apply_notification(&dsh_pager_protocol::JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "generation": 2,
                    "type": "session/jobs",
                    "sessionId": "s",
                    "jobs": [{"id": "job", "status": "running"}]
                })),
            })
            .unwrap();
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane(store.snapshots().cloned().collect());
        let row = dashboard.rows()[0];
        assert_eq!(row.status, DashboardStatus::Failed);
        assert_eq!(row.jobs, 1);
        assert_eq!(row.running_jobs, 1);
        assert_eq!(row.stopping_jobs, 0);
        assert_eq!(row.cwd.as_deref(), Some("/work"));
    }

    #[test]
    fn workspace_baseline_groups_rows_and_keeps_ungrouped_sessions_visible() {
        let mut first = snapshot("first", 1);
        first.workspace_id = Some("w1".into());
        first.cwd = Some("/one".into());
        let mut second = snapshot("second", 2);
        second.workspace_id = Some("w2".into());
        let third = snapshot("third", 3);
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane_with_workspaces(
            vec![first, second, third],
            vec![
                WorkspaceView {
                    workspace_id: "w2".into(),
                    path: "/two".into(),
                    title: "Two".into(),
                    session_ids: vec!["second".into()],
                    created_at: String::new(),
                    updated_at: String::new(),
                    raw: Value::Null,
                },
                WorkspaceView {
                    workspace_id: "w1".into(),
                    path: "/one".into(),
                    title: "One".into(),
                    session_ids: vec!["first".into()],
                    created_at: String::new(),
                    updated_at: String::new(),
                    raw: Value::Null,
                },
            ],
            vec!["w1".into(), "w2".into()],
        );
        assert_eq!(dashboard.workspaces()[0].workspace_id, "w1");
        let rows = dashboard.rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].workspace_id.as_deref(), Some("w1"));
        assert_eq!(rows[0].workspace_title.as_deref(), Some("One"));
        assert!(rows.iter().any(|row| row.workspace_id.is_none()));
    }

    #[test]
    fn workspace_session_ids_define_order_before_activity_tiebreakers() {
        let mut older = snapshot("older", 1);
        older.workspace_id = Some("w".into());
        let mut newer = snapshot("newer", 99);
        newer.workspace_id = Some("w".into());
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane_with_workspaces(
            vec![older, newer],
            vec![WorkspaceView {
                workspace_id: "w".into(),
                path: "/work".into(),
                title: "Work".into(),
                session_ids: vec!["older".into(), "newer".into()],
                created_at: String::new(),
                updated_at: String::new(),
                raw: Value::Null,
            }],
            vec!["w".into()],
        );
        assert_eq!(
            dashboard
                .rows()
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["older", "newer"]
        );
    }

    #[test]
    fn disabling_workspace_groups_uses_global_activity_order() {
        let mut first = snapshot("first", 1);
        first.workspace_id = Some("w1".into());
        let mut second = snapshot("second", 99);
        second.workspace_id = Some("w2".into());
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane_with_workspaces(
            vec![first, second],
            vec![
                WorkspaceView {
                    workspace_id: "w1".into(),
                    path: String::new(),
                    title: "One".into(),
                    session_ids: vec!["first".into()],
                    created_at: String::new(),
                    updated_at: String::new(),
                    raw: Value::Null,
                },
                WorkspaceView {
                    workspace_id: "w2".into(),
                    path: String::new(),
                    title: "Two".into(),
                    session_ids: vec!["second".into()],
                    created_at: String::new(),
                    updated_at: String::new(),
                    raw: Value::Null,
                },
            ],
            vec!["w1".into(), "w2".into()],
        );
        dashboard.set_group_by_workspace(false);
        assert_eq!(dashboard.rows()[0].session_id, "second");
    }

    #[test]
    fn archived_and_inactive_filters_are_reversible() {
        let mut archived = snapshot("archived", 1);
        archived.archived = true;
        let mut inactive = snapshot("inactive", 2);
        inactive.blank = Some(false);
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane(vec![archived, inactive]);
        dashboard.set_show_archived(false);
        assert_eq!(dashboard.rows().len(), 1);
        assert_eq!(dashboard.rows()[0].session_id, "inactive");
        dashboard.set_collapse_inactive(true);
        assert!(dashboard.rows().is_empty());
        dashboard.toggle_collapse_inactive();
        dashboard.toggle_show_archived();
        assert_eq!(dashboard.rows().len(), 2);
    }

    #[test]
    fn selected_removed_row_remains_visible_as_a_gone_placeholder() {
        let mut removed = snapshot("gone", 1);
        removed.removed = true;
        let live = snapshot("live", 2);
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane(vec![removed, live]);
        dashboard.select("gone");
        assert!(dashboard.rows().iter().any(|row| row.session_id == "gone"));
        dashboard.select("live");
        assert!(!dashboard.rows().iter().any(|row| row.session_id == "gone"));
    }

    #[test]
    fn session_reorder_uses_host_order_even_when_rows_are_filtered() {
        let mut first = snapshot("first", 1);
        first.workspace_id = Some("w".into());
        let mut second = snapshot("second", 2);
        second.workspace_id = Some("w".into());
        second.archived = true;
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane_with_workspaces(
            vec![first, second],
            vec![WorkspaceView {
                workspace_id: "w".into(),
                path: "/work".into(),
                title: "Work".into(),
                session_ids: vec!["second".into(), "first".into()],
                created_at: String::new(),
                updated_at: String::new(),
                raw: Value::Null,
            }],
            vec!["w".into()],
        );
        dashboard.set_show_archived(false);
        assert_eq!(dashboard.session_ids_in_workspace("w"), ["second", "first"]);
    }

    #[test]
    fn parent_collapse_and_status_filter_use_a_stable_fallback() {
        let mut parent = snapshot("parent", 1);
        parent.running = Some(true);
        let mut child = snapshot("child", 2);
        child.parent_session_id = Some("parent".into());
        let mut dashboard = DashboardModel::default();
        dashboard.replace_control_plane(vec![parent, child]);
        dashboard.select("child");
        dashboard.toggle_session_tree("parent");
        assert_eq!(
            dashboard
                .rows()
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["parent"]
        );
        dashboard.toggle_session_tree("parent");
        dashboard.set_status_filter(Some(DashboardStatus::Running));
        assert_eq!(
            dashboard
                .rows()
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["parent"]
        );
        dashboard.set_status_filter(None);
        assert_eq!(dashboard.selected_id(), Some("parent"));
    }

    #[test]
    fn row_action_state_machine_rejects_duplicates_and_stale_results() {
        let mut dashboard = DashboardModel::default();
        assert!(dashboard.begin_action("s", DashboardActionKind::Reply));
        dashboard.mark_action_pending("s", DashboardActionKind::Reply, "r1".into(), 4);
        assert!(!dashboard.begin_action("s", DashboardActionKind::Reply));
        dashboard.resolve_action_for_request(
            "s",
            DashboardActionKind::Reply,
            Some("r-old"),
            4,
            true,
            None,
        );
        assert_eq!(
            dashboard.action_state("s", DashboardActionKind::Reply),
            DashboardActionState::Stale
        );
        assert!(dashboard.begin_action("s", DashboardActionKind::Reply));
        dashboard.resolve_action(
            "s",
            DashboardActionKind::Reply,
            4,
            false,
            Some("rejected".into()),
        );
        assert_eq!(
            dashboard.action_state("s", DashboardActionKind::Reply),
            DashboardActionState::Rejected("rejected".into())
        );
    }
}
