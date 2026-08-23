//! Workspace tree focus adapter for the Grok dashboard surface.
//!
//! DSH's `DashboardModel` owns filtering, grouping and mutation lifecycle.
//! This small controller keeps the workspace focus as a stable workspace id so
//! a control-plane refresh cannot make a focused group point at another row.

use dsh_pager::DashboardModel;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkspaceTreeController {
    focused_workspace_id: Option<String>,
}

impl WorkspaceTreeController {
    pub fn clear(&mut self) {
        self.focused_workspace_id = None;
    }

    pub fn sync(&mut self, model: &DashboardModel) {
        let selected_workspace = model.selected().and_then(|row| row.workspace_id.clone());
        self.focused_workspace_id = selected_workspace.filter(|id| {
            model
                .workspaces()
                .iter()
                .any(|workspace| workspace.workspace_id == *id)
        });
    }

    pub fn focused_workspace_id(&self) -> Option<&str> {
        self.focused_workspace_id.as_deref()
    }

    pub fn focus(&mut self, workspace_id: &str, model: &DashboardModel) -> bool {
        if model
            .workspaces()
            .iter()
            .any(|workspace| workspace.workspace_id == workspace_id)
        {
            self.focused_workspace_id = Some(workspace_id.to_string());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::dashboard::DashboardModel;

    #[test]
    fn refresh_drops_workspace_focus_when_id_disappears() {
        let mut controller = WorkspaceTreeController {
            focused_workspace_id: Some("ws-old".into()),
        };
        let model = DashboardModel::default();
        controller.sync(&model);
        assert_eq!(controller.focused_workspace_id(), None);
    }
}
