//! Changes-view conversion sub-handlers for `PullRequestsMessage`.

use std::ops::ControlFlow;

use crate::state::AppEvent;

use super::PullRequestsMessage;

impl PullRequestsMessage {
    pub(super) fn changes_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        ControlFlow::Break(match event {
            AppEvent::PrOpenChanges => Self::OpenChanges,
            AppEvent::PrChangesFocusContent => Self::ChangesFocusContent,
            AppEvent::PrChangesFocusFiles => Self::ChangesFocusFiles,
            AppEvent::PrChangesToggleView => Self::ChangesToggleView,
            AppEvent::PrOpenChangesComment => Self::OpenChangesComment,
            AppEvent::PrChangesBack => Self::ChangesBack,
            AppEvent::PrChangesRetryFiles => Self::ChangesRetryFiles,
            AppEvent::PrChangesRetryBlob => Self::ChangesRetryBlob,
            AppEvent::PrChangesLoaded(payload) => Self::ChangesLoaded(payload),
            AppEvent::PrChangesLoadFailed(payload) => Self::ChangesLoadFailed(payload),
            AppEvent::PrChangesBlobLoaded(payload) => Self::ChangesBlobLoaded(payload),
            AppEvent::PrChangesBlobLoadFailed(payload) => Self::ChangesBlobLoadFailed(payload),
            other => return ControlFlow::Continue(other),
        })
    }

    pub(super) fn changes_into_app_event(self) -> ControlFlow<AppEvent, Self> {
        ControlFlow::Break(match self {
            Self::OpenChanges => AppEvent::PrOpenChanges,
            Self::ChangesFocusContent => AppEvent::PrChangesFocusContent,
            Self::ChangesFocusFiles => AppEvent::PrChangesFocusFiles,
            Self::ChangesToggleView => AppEvent::PrChangesToggleView,
            Self::OpenChangesComment => AppEvent::PrOpenChangesComment,
            Self::ChangesBack => AppEvent::PrChangesBack,
            Self::ChangesRetryFiles => AppEvent::PrChangesRetryFiles,
            Self::ChangesRetryBlob => AppEvent::PrChangesRetryBlob,
            Self::ChangesLoaded(payload) => AppEvent::PrChangesLoaded(payload),
            Self::ChangesLoadFailed(payload) => AppEvent::PrChangesLoadFailed(payload),
            Self::ChangesBlobLoaded(payload) => AppEvent::PrChangesBlobLoaded(payload),
            Self::ChangesBlobLoadFailed(payload) => AppEvent::PrChangesBlobLoadFailed(payload),
            other => return ControlFlow::Continue(other),
        })
    }
}
