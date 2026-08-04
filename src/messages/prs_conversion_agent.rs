use crate::state::AppEvent;

use super::{NavDir, PullRequestsMessage};

impl PullRequestsMessage {
    pub(super) fn into_app_event_agent(self) -> AppEvent {
        match self {
            Self::OpenAgentChooser { metadata } => AppEvent::PrOpenAgentChooser { metadata },
            Self::BeginListSendDetail { metadata } => AppEvent::BeginPrListSendDetail(metadata),
            Self::CancelListSendDetail => AppEvent::CancelPrListSendDetail,
            Self::ListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            } => AppEvent::PrListSendDetailReady {
                scope_repo_id,
                pr_number,
                request_id,
            },
            Self::AgentChooserNavigate(NavDir::Up) => AppEvent::PrAgentChooserNavigateUp,
            Self::AgentChooserNavigate(NavDir::Down) => AppEvent::PrAgentChooserNavigateDown,
            Self::AgentChooserConfirm => AppEvent::PrAgentChooserConfirm,
            Self::AgentChooserCancel => AppEvent::PrAgentChooserCancel,
            Self::SendToAgentCompleted => AppEvent::PrSendToAgentCompleted,
            Self::SendToAgentFailed { error } => AppEvent::PrSendToAgentFailed { error },
            other => other.into_app_event_lifecycle(),
        }
    }

    pub(super) fn is_pr_property_message(message: &Self) -> bool {
        matches!(
            message,
            Self::OpenPropertyEditor { .. }
                | Self::PropertyEditorNavigateUp
                | Self::PropertyEditorNavigateDown
                | Self::PropertyEditorToggle
                | Self::PropertyEditorConfirm
                | Self::PropertyEditorCancel
                | Self::PropertyEditorTitleChar(_)
                | Self::PropertyEditorTitleBackspace
                | Self::PropertyEditorTitleDelete
                | Self::PropertyEditorTitleCursorLeft
                | Self::PropertyEditorTitleCursorRight
                | Self::PropertyEditorTitleCursorHome
                | Self::PropertyEditorTitleCursorEnd
                | Self::PropertyEditorOptionsLoaded { .. }
                | Self::PropertyEditorOptionsFailed { .. }
                | Self::PropertyEditSucceeded { .. }
                | Self::PostMutationRefreshStarted
                | Self::PropertyEditFailed { .. }
                | Self::PropertyEditorValidationError { .. }
        )
    }
}
