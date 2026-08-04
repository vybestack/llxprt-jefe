//! Reducers for the multi-agent workbench (issue #626).
//!
//! These live beside the rest of the workbench state rather than in
//! `state/mod.rs` so the central reducer file stays inside the source-size
//! gate. They are pure: they mutate only `AppState` and perform no I/O.

use super::AppState;
use super::workbench_filter::WorkbenchStatusFilter;
use crate::messages::UiNavigationMessage;
use crate::workbench_view::StatusBucket;

/// The filter rail lists the buckets in this order, top to bottom.
const FILTER_ORDER: [StatusBucket; 4] = [
    StatusBucket::NeedsYou,
    StatusBucket::Working,
    StatusBucket::Ready,
    StatusBucket::Stale,
];

impl UiNavigationMessage {
    /// Whether this message belongs to the multi-agent workbench.
    pub(super) const fn is_workbench(&self) -> bool {
        matches!(
            self,
            Self::ToggleWorkbenchStatusBucket(_)
                | Self::WorkbenchNextPage
                | Self::WorkbenchPrevPage
                | Self::WorkbenchFilterCursorPrev
                | Self::WorkbenchFilterCursorNext
                | Self::WorkbenchSelectPrev
                | Self::WorkbenchSelectNext
                | Self::WorkbenchAttach
        )
    }
}

impl AppState {
    /// Move the agent selection one card along the workbench's own order.
    ///
    /// The workbench does not keep a second selection: it moves the app's
    /// existing selected agent, so `Enter` and the dashboard agree about which
    /// agent is current. Ordering comes from the projection, so the selection
    /// walks the cards exactly as they are rendered.
    pub(super) fn move_workbench_selection(&mut self, forward: bool) {
        let inputs: Vec<_> = self
            .agents
            .iter()
            .map(|agent| crate::workbench_view::AgentInput {
                agent,
                git_info: None,
                observation: self.observations.get(&agent.id),
            })
            .collect();
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
    fn select_workbench_agent(&mut self, target: &crate::domain::AgentId) {
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
        FILTER_ORDER[self.workbench.filter_cursor.min(FILTER_ORDER.len() - 1)]
    }

    /// Handle multi-agent workbench navigation messages.
    ///
    /// Paging deliberately has no upper bound here. The number of pages depends
    /// on terminal size, which is a render-time fact and is not part of
    /// `AppState`, so the projection clamps the requested page against the real
    /// page count when it builds the view.
    pub(super) fn apply_workbench_navigation(&mut self, message: UiNavigationMessage) {
        match message {
            UiNavigationMessage::ToggleWorkbenchStatusBucket(bucket) => {
                self.apply_workbench_status_toggle(bucket);
            }
            UiNavigationMessage::WorkbenchNextPage => {
                self.workbench.page = self.workbench.page.saturating_add(1);
            }
            UiNavigationMessage::WorkbenchPrevPage => {
                self.workbench.page = self.workbench.page.saturating_sub(1);
            }
            UiNavigationMessage::WorkbenchFilterCursorPrev => {
                self.workbench.filter_cursor = self.workbench.filter_cursor.saturating_sub(1);
            }
            UiNavigationMessage::WorkbenchFilterCursorNext => {
                self.workbench.filter_cursor =
                    (self.workbench.filter_cursor + 1).min(FILTER_ORDER.len() - 1);
            }
            UiNavigationMessage::WorkbenchSelectPrev => self.move_workbench_selection(false),
            UiNavigationMessage::WorkbenchSelectNext => self.move_workbench_selection(true),
            UiNavigationMessage::WorkbenchAttach => self.attach_workbench_selection(),
            _ => unreachable!("non-workbench message routed to apply_workbench_navigation"),
        }
    }

    /// Toggle one status bucket in the workbench filter mask and reset the page
    /// to 0, so a shrinking list cannot strand the view on an empty page.
    fn apply_workbench_status_toggle(&mut self, bucket: StatusBucket) {
        let current = self.workbench.status_filter.mask();
        self.workbench.status_filter =
            WorkbenchStatusFilter(current.with(bucket, !current.allows(bucket)));
        self.workbench.page = 0;
    }
}
