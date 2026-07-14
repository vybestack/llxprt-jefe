//! `message_names!` macro and all its invocations, extracted from
//! `messages.rs` to keep that file under the per-file line limit.

use super::{
    IssuesMessage, ModalMessage, PersistenceMessage, PullRequestsMessage, RepositoryAgentMessage,
    RuntimeMessage, SystemMessage, ThemeMessage, UiNavigationMessage,
};

macro_rules! message_names {
    ($enum_name:ident { $($variant:pat => $name:literal),+ $(,)? }) => {
        impl $enum_name {
            #[must_use]
            pub const fn name(&self) -> &'static str {
                match self {
                    $($variant => $name,)+
                }
            }
        }
    };
}

message_names!(UiNavigationMessage {
    Self::NavigateUp => "NavigateUp",
    Self::NavigateDown => "NavigateDown",
    Self::NavigateLeft => "NavigateLeft",
    Self::NavigateRight => "NavigateRight",
    Self::SelectRepository(_) => "SelectRepository",
    Self::SelectAgent(_) => "SelectAgent",
    Self::JumpToAgentByShortcut(_) => "JumpToAgentByShortcut",
    Self::CyclePaneFocus => "CyclePaneFocus",
    Self::ToggleTerminalFocus => "ToggleTerminalFocus",
    Self::ToggleHideIdleRepositories => "ToggleHideIdleRepositories",
    Self::EnterSplitMode => "EnterSplitMode",
    Self::ExitSplitMode => "ExitSplitMode",
    Self::EnterGrabMode => "EnterGrabMode",
    Self::ExitGrabMode => "ExitGrabMode",
    Self::GrabMoveUp => "GrabMoveUp",
    Self::GrabMoveDown => "GrabMoveDown",
    Self::SetSplitFilter(_) => "SetSplitFilter",
    Self::EnterDashboardGrab => "EnterDashboardGrab",
    Self::ExitDashboardGrab => "ExitDashboardGrab",
    Self::DashboardGrabMoveUp => "DashboardGrabMoveUp",
    Self::DashboardGrabMoveDown => "DashboardGrabMoveDown",
    Self::TerminalScrollUp => "TerminalScrollUp",
    Self::TerminalScrollDown => "TerminalScrollDown",
    Self::TerminalScrollPageUp => "TerminalScrollPageUp",
    Self::TerminalScrollPageDown => "TerminalScrollPageDown",
    Self::TerminalFollowTail => "TerminalFollowTail",
    Self::TerminalScrollToTop => "TerminalScrollToTop",
});

message_names!(ModalMessage {
    Self::OpenHelp => "OpenHelp",
    Self::OpenSearch => "OpenSearch",
    Self::CloseModal => "CloseModal",
    Self::SubmitForm => "SubmitForm",
    Self::ConfirmCycleFocus => "ConfirmCycleFocus",
    Self::FormChar(_) => "FormChar",
    Self::FormBackspace => "FormBackspace",
    Self::FormDelete => "FormDelete",
    Self::FormMoveCursorLeft => "FormMoveCursorLeft",
    Self::FormMoveCursorRight => "FormMoveCursorRight",
    Self::FormNextField => "FormNextField",
    Self::FormPrevField => "FormPrevField",
    Self::FormToggleCheckbox => "FormToggleCheckbox",
});

message_names!(RepositoryAgentMessage {
    Self::OpenNewRepository => "OpenNewRepository",
    Self::OpenEditRepository(_) => "OpenEditRepository",
    Self::OpenDeleteRepository(_) => "OpenDeleteRepository",
    Self::OpenNewAgent(_) => "OpenNewAgent",
    Self::OpenEditAgent(_) => "OpenEditAgent",
    Self::OpenDeleteAgent(_) => "OpenDeleteAgent",
    Self::ToggleDeleteWorkDir => "ToggleDeleteWorkDir",
});

message_names!(RuntimeMessage {
    Self::KillAgent(_) => "KillAgent",
    Self::RelaunchAgent(_) => "RelaunchAgent",
    Self::RestartAgent(_) => "RestartAgent",
    Self::AgentStatusChanged(_, _) => "AgentStatusChanged",
});

message_names!(PersistenceMessage {
    Self::LoadSuccess => "PersistenceLoadSuccess",
    Self::LoadFailed(_) => "PersistenceLoadFailed",
    Self::SaveSuccess => "PersistenceSaveSuccess",
    Self::SaveFailed(_) => "PersistenceSaveFailed",
});

message_names!(ThemeMessage {
    Self::SetTheme(_) => "SetTheme",
    Self::ResolveFailed(_) => "ThemeResolveFailed",
    Self::OpenThemePicker { .. } => "OpenThemePicker",
    Self::PickerNavigateUp => "ThemePickerNavigateUp",
    Self::PickerNavigateDown => "ThemePickerNavigateDown",
    Self::PickerConfirm => "ThemePickerConfirm",
    Self::PickerCancel => "CloseThemePicker",
    Self::ToggleAgentThemeOverride => "ThemePickerToggleOverride",
});

message_names!(SystemMessage {
    Self::Quit => "Quit",
    Self::ClearError => "ClearError",
    Self::ClearWarning => "ClearWarning",
});

message_names!(IssuesMessage {
    Self::EnterMode => "EnterIssuesMode",
    Self::ExitMode => "ExitIssuesMode",
    Self::RefocusList => "RefocusIssueList",
    Self::NavigateUp => "IssuesNavigateUp",
    Self::NavigateDown => "IssuesNavigateDown",
    Self::NavigatePageUp => "IssuesNavigatePageUp",
    Self::NavigatePageDown => "IssuesNavigatePageDown",
    Self::NavigateHome => "IssuesNavigateHome",
    Self::NavigateEnd => "IssuesNavigateEnd",
    Self::Enter => "IssuesEnter",
    Self::CycleFocus => "IssuesCycleFocus",
    Self::CycleFocusReverse => "IssuesCycleFocusReverse",
    Self::ScrollDetailUp => "IssuesScrollDetailUp",
    Self::ScrollDetailDown => "IssuesScrollDetailDown",
    Self::ScrollDetailPageUp => "IssuesScrollDetailPageUp",
    Self::ScrollDetailPageDown => "IssuesScrollDetailPageDown",
    Self::DetailSubfocusNext => "IssueDetailSubfocusNext",
    Self::DetailSubfocusPrev => "IssueDetailSubfocusPrev",
    Self::ListLoaded { .. } => "IssueListLoaded",
    Self::ListLoadFailed { .. } => "IssueListLoadFailed",
    Self::ListPageLoaded { .. } => "IssueListPageLoaded",
    Self::DetailLoaded { .. } => "IssueDetailLoaded",
    Self::DetailLoadFailed { .. } => "IssueDetailLoadFailed",
    Self::CommentsPageLoaded { .. } => "IssueCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "IssueCommentsPageFailed",
    Self::ListSilentRefreshed { .. } => "IssueListSilentRefreshed",
    Self::ListSilentRefreshFailed { .. } => "IssueListSilentRefreshFailed",
    Self::DetailSilentRefreshed { .. } => "IssueDetailSilentRefreshed",
    Self::DetailSilentRefreshFailed { .. } => "IssueDetailSilentRefreshFailed",
    Self::OpenFilterControls => "OpenFilterControls",
    Self::CloseFilterControls => "CloseFilterControls",
    Self::ApplyFilter => "ApplyFilter",
    Self::ClearFilter => "ClearFilter",
    Self::ClearDraftFilter => "ClearDraftFilter",
    Self::FilterNavigateNext => "FilterNavigateNext",
    Self::FilterNavigatePrev => "FilterNavigatePrev",
    Self::CycleFilterState => "CycleFilterState",
    Self::FocusSearchInput => "FocusSearchInput",
    Self::BlurSearchInput => "BlurSearchInput",
    Self::SetSearchQuery { .. } => "SetSearchQuery",
    Self::ApplySearch => "ApplySearch",
    Self::ClearSearch => "ClearSearch",
    Self::UpdateDraftFilter { .. } => "UpdateDraftFilter",
    Self::OpenNewIssueComposer => "OpenNewIssueComposer",
    Self::OpenNewCommentComposer => "OpenNewCommentComposer",
    Self::OpenReplyComposer { .. } => "OpenReplyComposer",
    Self::OpenInlineEditor { .. } => "OpenInlineEditor",
    Self::InlineChar(_) => "InlineChar",
    Self::InlineNewline => "InlineNewline",
    Self::InlineBackspace => "InlineBackspace",
    Self::InlineDelete => "InlineDelete",
    Self::InlineCursorLeft => "InlineCursorLeft",
    Self::InlineCursorRight => "InlineCursorRight",
    Self::InlineCursorUp => "InlineCursorUp",
    Self::InlineCursorDown => "InlineCursorDown",
    Self::InlineSubmit => "InlineSubmit",
    Self::InlineCancelOrEsc => "InlineCancelOrEsc",
    Self::MutationSubmitted { .. } => "MutationSubmitted",
    Self::IssueCreated { .. } => "IssueCreated",
    Self::CommentCreated { .. } => "CommentCreated",
    Self::CommentCreateFailed { .. } => "CommentCreateFailed",
    Self::IssueBodyUpdated { .. } => "IssueBodyUpdated",
    Self::CommentUpdated { .. } => "CommentUpdated",
    Self::MutationFailed { .. } => "MutationFailed",
    Self::OpenAgentChooser { .. } => "OpenAgentChooser",
    Self::AgentChooserNavigateUp => "AgentChooserNavigateUp",
    Self::AgentChooserNavigateDown => "AgentChooserNavigateDown",
    Self::AgentChooserConfirm => "AgentChooserConfirm",
    Self::AgentChooserCancel => "AgentChooserCancel",
    Self::SendToAgentCompleted => "SendToAgentCompleted",
    Self::SendToAgentFailed { .. } => "SendToAgentFailed",
    Self::IssueSelfAssignmentFailed { .. } => "IssueSelfAssignmentFailed",
    Self::OpenPropertyEditor { .. } => "IssueOpenPropertyEditor",
    Self::PropertyEditorNavigateUp => "IssuePropertyEditorNavigateUp",
    Self::PropertyEditorNavigateDown => "IssuePropertyEditorNavigateDown",
    Self::PropertyEditorToggle => "IssuePropertyEditorToggle",
    Self::PropertyEditorConfirm => "IssuePropertyEditorConfirm",
    Self::PropertyEditorCancel => "IssuePropertyEditorCancel",
    Self::PropertyEditorTitleChar(_) => "IssuePropertyEditorTitleChar",
    Self::PropertyEditorTitleBackspace => "IssuePropertyEditorTitleBackspace",
    Self::PropertyEditorTitleDelete => "IssuePropertyEditorTitleDelete",
    Self::PropertyEditorTitleCursorLeft => "IssuePropertyEditorTitleCursorLeft",
    Self::PropertyEditorTitleCursorRight => "IssuePropertyEditorTitleCursorRight",
    Self::PropertyEditorOptionsLoaded { .. } => "IssuePropertyEditorOptionsLoaded",
    Self::PropertyEditorOptionsFailed { .. } => "IssuePropertyEditorOptionsFailed",
    Self::PropertyEditSucceeded { .. } => "IssuePropertyEditSucceeded",
    Self::PostMutationRefreshStarted => "IssuePostMutationRefreshStarted",
    Self::PropertyEditFailed { .. } => "IssuePropertyEditFailed",
    Self::PropertyEditorValidationError { .. } => "IssuePropertyEditorValidationError",
});

// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
// @pseudocode component-004 lines 43-44
message_names!(PullRequestsMessage {
    Self::EnterMode => "EnterPrsMode",
    Self::ExitMode => "ExitPrsMode",
    Self::RefocusList => "RefocusPrList",
    Self::Navigate(_) => "PrNavigate",
    Self::Enter => "PrListEnter",
    Self::CycleFocus => "PrCycleFocus",
    Self::CycleFocusReverse => "PrCycleFocusReverse",
    Self::ScrollDetail(_) => "PrScrollDetail",
    Self::DetailSubfocusNext => "PrDetailSubfocusNext",
    Self::DetailSubfocusPrev => "PrDetailSubfocusPrev",
    Self::ListLoaded { .. } => "PrListLoaded",
    Self::ListLoadFailed { .. } => "PrListLoadFailed",
    Self::ListPageLoaded { .. } => "PrListPageLoaded",
    Self::ListSilentRefreshed { .. } => "PrListSilentRefreshed",
    Self::ListSilentRefreshFailed { .. } => "PrListSilentRefreshFailed",
    Self::DetailLoaded { .. } => "PrDetailLoaded",
    Self::DetailLoadFailed { .. } => "PrDetailLoadFailed",
    Self::DetailSilentRefreshed { .. } => "PrDetailSilentRefreshed",
    Self::DetailSilentRefreshFailed { .. } => "PrDetailSilentRefreshFailed",
    Self::CommentsPageLoaded { .. } => "PrCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "PrCommentsPageFailed",
    Self::OpenFilterControls => "PrOpenFilterControls",
    Self::CloseFilterControls => "PrCloseFilterControls",
    Self::ApplyFilter => "PrApplyFilter",
    Self::ClearFilter => "PrClearFilter",
    Self::FilterNavigate(_) => "PrFilterNavigate",
    Self::CycleFilterState => "PrCycleFilterState",
    Self::CycleDraftFilter => "PrCycleDraftFilter",
    Self::CycleReviewFilter => "PrCycleReviewFilter",
    Self::CycleChecksFilter => "PrCycleChecksFilter",
    Self::UpdateDraftFilter { .. } => "PrUpdateDraftFilter",
    Self::FocusSearchInput => "PrFocusSearchInput",
    Self::BlurSearchInput => "PrBlurSearchInput",
    Self::SetSearchQuery { .. } => "PrSetSearchQuery",
    Self::ApplySearch => "PrApplySearch",
    Self::ClearSearch => "PrClearSearch",
    Self::OpenNewCommentComposer => "PrOpenNewCommentComposer",
    Self::OpenReplyComposer { .. } => "PrOpenReplyComposer",
    Self::Inline(_) => "PrInline",
    Self::CommentCreated { .. } => "PrCommentCreated",
    Self::CommentCreateFailed { .. } => "PrCommentCreateFailed",
    Self::MutationFailed { .. } => "PrMutationFailed",
    Self::ShowNotice(_) => "PrShowNotice",
    Self::OpenAgentChooser { .. } => "PrOpenAgentChooser",
    Self::AgentChooserNavigate(_) => "PrAgentChooserNavigate",
    Self::AgentChooserConfirm => "PrAgentChooserConfirm",
    Self::AgentChooserCancel => "PrAgentChooserCancel",
    Self::SendToAgentCompleted => "PrSendToAgentCompleted",
    Self::SendToAgentFailed { .. } => "PrSendToAgentFailed",
    Self::OpenInBrowser => "PrOpenInBrowser",
    Self::OpenedInBrowser { .. } => "PrOpenedInBrowser",
    Self::OpenInBrowserFailed { .. } => "PrOpenInBrowserFailed",
    Self::OpenMergeChooser => "PrOpenMergeChooser",
    Self::MergeNavigate(_) => "PrMergeNavigate",
    Self::MergeConfirm => "PrMergeConfirm",
    Self::MergeCancel => "PrMergeCancel",
    Self::Merged { .. } => "PrMerged",
    Self::MergeFailed { .. } => "PrMergeFailed",
    Self::MergeMethodsLoaded { .. } => "PrMergeMethodsLoaded",
    Self::OpenThreadReply { .. } => "PrOpenThreadReply",
    Self::ToggleThreadResolve { .. } => "PrToggleThreadResolve",
    Self::ThreadResolveSucceeded { .. } => "PrThreadResolveSucceeded",
    Self::ThreadResolveFailed { .. } => "PrThreadResolveFailed",
    Self::OpenPropertyEditor { .. } => "PrOpenPropertyEditor",
    Self::PropertyEditorNavigateUp => "PrPropertyEditorNavigateUp",
    Self::PropertyEditorNavigateDown => "PrPropertyEditorNavigateDown",
    Self::PropertyEditorToggle => "PrPropertyEditorToggle",
    Self::PropertyEditorConfirm => "PrPropertyEditorConfirm",
    Self::PropertyEditorCancel => "PrPropertyEditorCancel",
    Self::PropertyEditorTitleChar(_) => "PrPropertyEditorTitleChar",
    Self::PropertyEditorTitleBackspace => "PrPropertyEditorTitleBackspace",
    Self::PropertyEditorTitleDelete => "PrPropertyEditorTitleDelete",
    Self::PropertyEditorTitleCursorLeft => "PrPropertyEditorTitleCursorLeft",
    Self::PropertyEditorTitleCursorRight => "PrPropertyEditorTitleCursorRight",
    Self::PropertyEditorOptionsLoaded { .. } => "PrPropertyEditorOptionsLoaded",
    Self::PropertyEditorOptionsFailed { .. } => "PrPropertyEditorOptionsFailed",
    Self::PropertyEditSucceeded { .. } => "PrPropertyEditSucceeded",
    Self::PostMutationRefreshStarted => "PrPostMutationRefreshStarted",
    Self::PropertyEditFailed { .. } => "PrPropertyEditFailed",
    Self::PropertyEditorValidationError { .. } => "PrPropertyEditorValidationError",
});

// ── Property-editor predicate guards (issue #175) ──────────────────────────
//
// Used as match guards by the controls dispatchers in `issues_conversion.rs`
// and `prs_property_conversion.rs` so those functions do not need to list every
// property variant inline (keeping them under the per-function line budget).

use crate::state::AppEvent;

/// Whether an `AppEvent` is an issue property-editor event (issue #175).
#[must_use]
pub(super) fn is_issue_property_app_event(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::IssueOpenPropertyEditor { .. }
            | AppEvent::IssuePropertyEditorNavigateUp
            | AppEvent::IssuePropertyEditorNavigateDown
            | AppEvent::IssuePropertyEditorToggle
            | AppEvent::IssuePropertyEditorConfirm
            | AppEvent::IssuePropertyEditorCancel
            | AppEvent::IssuePropertyEditorTitleChar(_)
            | AppEvent::IssuePropertyEditorTitleBackspace
            | AppEvent::IssuePropertyEditorTitleDelete
            | AppEvent::IssuePropertyEditorTitleCursorLeft
            | AppEvent::IssuePropertyEditorTitleCursorRight
            | AppEvent::IssuePropertyEditorOptionsLoaded { .. }
            | AppEvent::IssuePropertyEditorOptionsFailed { .. }
            | AppEvent::IssuePropertyEditSucceeded { .. }
            | AppEvent::IssuePostMutationRefreshStarted
            | AppEvent::IssuePropertyEditFailed { .. }
            | AppEvent::IssuePropertyEditorValidationError { .. }
    )
}

/// Whether an `IssuesMessage` is a property-editor message (issue #175).
#[must_use]
pub(super) fn is_issue_property_msg(message: &IssuesMessage) -> bool {
    matches!(
        message,
        IssuesMessage::OpenPropertyEditor { .. }
            | IssuesMessage::PropertyEditorNavigateUp
            | IssuesMessage::PropertyEditorNavigateDown
            | IssuesMessage::PropertyEditorToggle
            | IssuesMessage::PropertyEditorConfirm
            | IssuesMessage::PropertyEditorCancel
            | IssuesMessage::PropertyEditorTitleChar(_)
            | IssuesMessage::PropertyEditorTitleBackspace
            | IssuesMessage::PropertyEditorTitleDelete
            | IssuesMessage::PropertyEditorTitleCursorLeft
            | IssuesMessage::PropertyEditorTitleCursorRight
            | IssuesMessage::PropertyEditorOptionsLoaded { .. }
            | IssuesMessage::PropertyEditorOptionsFailed { .. }
            | IssuesMessage::PropertyEditSucceeded { .. }
            | IssuesMessage::PostMutationRefreshStarted
            | IssuesMessage::PropertyEditFailed { .. }
            | IssuesMessage::PropertyEditorValidationError { .. }
    )
}
