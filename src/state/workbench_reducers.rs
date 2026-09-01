//! Reducers for the multi-agent workbench (issue #626).
//!
//! These live beside the rest of the workbench state rather than in
//! `state/mod.rs` so the central reducer file stays inside the source-size
//! gate. They are pure: they mutate only `AppState` and perform no I/O.

use super::AppState;
use super::workbench_filter::WorkbenchStatusFilter;
use crate::workbench_view::StatusBucket;

pub(super) enum WorkbenchNavigation {
    ToggleStatusBucket(StatusBucket),
    NextPage,
    PreviousPage,
    PreviousFilter,
    NextFilter,
    PreviousSelection,
    NextSelection,
    Attach,
}

impl AppState {
    /// Move the agent selection one card along the workbench's own order.
    ///
    /// The workbench does not keep a second selection: it moves the app's
    /// existing selected agent, so `Enter` and the dashboard agree about which
    /// agent is current. Ordering comes from the projection, so the selection
    /// walks the cards exactly as they are rendered.
    pub(super) fn move_workbench_selection(&mut self, forward: bool) {
        let inputs = crate::host_panel_models::workbench_agent_inputs(self);
        let order = crate::workbench_view::ordered_agent_ids(
            &inputs,
            self.workbench.status_filter.mask(),
            None,
        );
        if order.is_empty() {
            return;
        }
        let current = self
            .selected_agent()
            .and_then(|agent| order.iter().position(|id| **id == agent.id));
        let next = match current {
            // No selection yet: enter the grid at whichever end the key implies.
            None if forward => 0,
            None => order.len() - 1,
            Some(index) if forward => (index + 1).min(order.len() - 1),
            Some(index) => index.saturating_sub(1),
        };
        let target = order[next].clone();
        self.select_workbench_agent(&target);
    }

    /// Attach to the selected card's agent: leave the workbench for the
    /// dashboard and put focus on that agent's terminal.
    ///
    /// Does nothing when no agent is selected, so Enter on an empty grid is
    /// inert rather than dropping the user on a dashboard with no terminal.
    fn attach_workbench_selection(&mut self) {
        if self.selected_agent().is_none() {
            return;
        }
        let _ = self.leave_screen();
        self.split_filter = None;
        self.split_grab_index = None;
        self.pane_focus = crate::state::PaneFocus::Terminal;
        self.terminal_focused = true;
    }

    /// Point the app's selection at `target`.
    ///
    /// The workbench spans repositories but `selected_agent` is repository
    /// scoped, so the repository has to move with the agent or the selection
    /// silently fails to resolve.
    pub(super) fn select_workbench_agent(&mut self, target: &crate::domain::AgentId) {
        let Some(agent_index) = self.agents.iter().position(|agent| agent.id == *target) else {
            return;
        };
        let repository_id = self.agents[agent_index].repository_id.clone();
        if let Some(repo_index) = self
            .repositories
            .iter()
            .position(|repository| repository.id == repository_id)
        {
            self.selected_repository_index = Some(repo_index);
        }
        self.selected_agent_index = Some(agent_index);
    }

    /// The bucket the filter cursor currently sits on.
    #[must_use]
    pub fn workbench_filter_cursor_bucket(&self) -> StatusBucket {
        crate::workbench_view::STATUS_BLOCK_ORDER[self
            .workbench
            .filter_cursor
            .min(crate::workbench_view::STATUS_BLOCK_ORDER.len() - 1)]
    }

    /// Handle multi-agent workbench navigation messages.
    ///
    /// Paging deliberately has no upper bound here. The number of pages depends
    /// on terminal size, which is a render-time fact and is not part of
    /// `AppState`, so the projection clamps the requested page against the real
    /// page count when it builds the view.
    pub(super) fn apply_workbench(&mut self, navigation: WorkbenchNavigation) {
        match navigation {
            WorkbenchNavigation::ToggleStatusBucket(bucket) => {
                self.apply_workbench_status_toggle(bucket);
            }
            WorkbenchNavigation::NextPage => {
                self.workbench.page = self.workbench.page.saturating_add(1);
            }
            WorkbenchNavigation::PreviousPage => {
                self.workbench.page = self.workbench.page.saturating_sub(1);
            }
            WorkbenchNavigation::PreviousFilter => {
                self.workbench.filter_cursor = self.workbench.filter_cursor.saturating_sub(1);
            }
            WorkbenchNavigation::NextFilter => {
                self.workbench.filter_cursor = (self.workbench.filter_cursor + 1)
                    .min(crate::workbench_view::STATUS_BLOCK_ORDER.len() - 1);
            }
            WorkbenchNavigation::PreviousSelection => self.move_workbench_selection(false),
            WorkbenchNavigation::NextSelection => self.move_workbench_selection(true),
            WorkbenchNavigation::Attach => self.attach_workbench_selection(),
        }
    }

    /// Advance the workbench page, clamped to the grid's real `page_count`.
    ///
    /// The host-panel input path knows the panel geometry, so unlike
    /// [`WorkbenchNavigation::NextPage`] it never lets the retained page
    /// counter advance past the last page the grid can show (issue #706);
    /// otherwise `PreviousPage` appears unresponsive until it walks back.
    pub(super) fn apply_workbench_page_next_within(&mut self, page_count: usize) {
        self.workbench.page = self
            .workbench
            .page
            .saturating_add(1)
            .min(page_count.saturating_sub(1));
    }

    /// Toggle one status bucket in the workbench filter mask and reset the page
    /// to 0, so a shrinking list cannot strand the view on an empty page.
    pub(super) fn apply_workbench_status_toggle(&mut self, bucket: StatusBucket) {
        let current = self.workbench.status_filter.mask();
        self.workbench.status_filter =
            WorkbenchStatusFilter(current.with(bucket, !current.allows(bucket)));
        self.workbench.page = 0;
    }
}
