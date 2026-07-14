use jefe::state::AppEvent;

#[derive(Clone, Copy)]
pub(super) enum SettledRefresh {
    Issues,
    PullRequests,
}

impl SettledRefresh {
    pub(super) fn from_event(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::IssueListLoaded { .. }
            | AppEvent::IssueListPageLoaded { .. }
            | AppEvent::IssueListLoadFailed { .. }
            | AppEvent::IssueListSilentRefreshed { .. }
            | AppEvent::IssueListSilentRefreshFailed { .. }
            | AppEvent::IssueDetailLoaded { .. }
            | AppEvent::IssueDetailLoadFailed { .. }
            | AppEvent::IssueDetailSilentRefreshed { .. }
            | AppEvent::IssueDetailSilentRefreshFailed { .. } => Some(Self::Issues),
            AppEvent::PrListLoaded { .. }
            | AppEvent::PrListPageLoaded { .. }
            | AppEvent::PrListLoadFailed { .. }
            | AppEvent::PrListSilentRefreshed { .. }
            | AppEvent::PrListSilentRefreshFailed { .. }
            | AppEvent::PrDetailLoaded { .. }
            | AppEvent::PrDetailLoadFailed { .. }
            | AppEvent::PrDetailSilentRefreshed { .. }
            | AppEvent::PrDetailSilentRefreshFailed { .. } => Some(Self::PullRequests),
            _ => None,
        }
    }
}
