//! Static name registration for the domain message enums.
//!
//! Each `message_names!` invocation generates a `pub const fn name(&self)`
//! returning a stable short label for tracing/logging. Extracted into its
//! own module to keep `messages.rs` focused on the enum definitions.

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
    Self::NavigatePageUp(_) => "NavigatePageUp",
    Self::NavigatePageDown(_) => "NavigatePageDown",
    Self::NavigateHome => "NavigateHome",
    Self::NavigateEnd => "NavigateEnd",
    Self::NavigateLeft => "NavigateLeft",
    Self::NavigateRight => "NavigateRight",
    Self::SelectRepository(_) => "SelectRepository",
    Self::SelectAgent(_) => "SelectAgent",
    Self::JumpToAgentByShortcut(_) => "JumpToAgentByShortcut",
    Self::CyclePaneFocus => "CyclePaneFocus",
    Self::ToggleTerminalFocus => "ToggleTerminalFocus",
    Self::ToggleHideIdleRepositories => "ToggleHideIdleRepositories",
    Self::FocusDashboardSearch => "FocusDashboardSearch",
    Self::BlurDashboardSearch => "BlurDashboardSearch",
    Self::SetDashboardSearchQuery { .. } => "SetDashboardSearchQuery",
    Self::ClearDashboardSearch => "ClearDashboardSearch",
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
    Self::OpenShellOverlay => "OpenShellOverlay",
    Self::CloseShellOverlay => "CloseShellOverlay",
    Self::HideShellOverlay => "HideShellOverlay",
    Self::ResumeShellOverlay(_) => "ResumeShellOverlay",
    Self::ToggleWorkbenchStatusBucket(_) => "ToggleWorkbenchStatusBucket",
    Self::WorkbenchNextPage => "WorkbenchNextPage",
    Self::WorkbenchPrevPage => "WorkbenchPrevPage",
    Self::WorkbenchFilterCursorPrev => "WorkbenchFilterCursorPrev",
    Self::WorkbenchFilterCursorNext => "WorkbenchFilterCursorNext",
    Self::WorkbenchSelectPrev => "WorkbenchSelectPrev",
    Self::WorkbenchSelectNext => "WorkbenchSelectNext",
    Self::WorkbenchAttach => "WorkbenchAttach",
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
    Self::FormMoveCursorStart => "FormMoveCursorStart",
    Self::FormMoveCursorEnd => "FormMoveCursorEnd",
    Self::FormNextField => "FormNextField",
    Self::FormPrevField => "FormPrevField",
    Self::FormToggleCheckbox => "FormToggleCheckbox",
});

message_names!(RepositoryAgentMessage {
    Self::OpenNewRepository => "OpenNewRepository",
    Self::OpenEditRepository(_) => "OpenEditRepository",
    Self::OpenDeleteRepository(_) => "OpenDeleteRepository",
    Self::OpenNewAgent(_) => "OpenNewAgent",
    Self::OpenAgentTypeForm(_) => "OpenAgentTypeForm",
    Self::OpenEditAgent(_) => "OpenEditAgent",
    Self::OpenDeleteAgent(_) => "OpenDeleteAgent",
    Self::ToggleDeleteWorkDir => "ToggleDeleteWorkDir",
    Self::ProbeAgentAvailability(_) => "ProbeAgentAvailability",
    Self::ProjectActionAvailability => "ProjectActionAvailability",
});

message_names!(RuntimeMessage {
    Self::KillAgent(_) => "KillAgent",
    Self::RelaunchAgent(_) => "RelaunchAgent",
    Self::RestartAgent(_) => "RestartAgent",
    Self::AgentStatusChanged(_, _) => "AgentStatusChanged",
    Self::ObservationUpdated(_, _, _) => "ObservationUpdated",
    Self::ObservationCleared(_, _) => "ObservationCleared",
});

message_names!(PersistenceMessage {
    Self::LoadSuccess => "PersistenceLoadSuccess",
    Self::LoadFailed(_) => "PersistenceLoadFailed",
    Self::SaveSuccess => "PersistenceSaveSuccess",
    Self::SaveFailed(_) => "PersistenceSaveFailed",
    Self::StageSave => "PersistenceStageSave",
});

message_names!(ThemeMessage {
    Self::ResolveFailed(_) => "ThemeResolveFailed",
});

message_names!(SystemMessage {
    Self::Quit => "Quit",
    Self::ClearError => "ClearError",
    Self::ClearWarning => "ClearWarning",
    Self::OpenAuthDialog => "OpenAuthDialog",
    Self::AuthCodeReceived { .. } => "AuthCodeReceived",
    Self::AuthSucceeded => "AuthSucceeded",
    Self::AuthFailed { .. } => "AuthFailed",
    Self::AuthCancelled => "AuthCancelled",
    Self::AuthRetry => "AuthRetry",
    Self::TransientAgentQueued { .. } => "TransientAgentQueued",
    Self::TransientAgentDequeued => "TransientAgentDequeued",
});

message_names!(IssuesMessage {
    Self::EnterMode => "EnterIssuesMode",
    Self::ExitMode => "ExitIssuesMode",
    Self::RefocusList => "RefocusIssueList",
    Self::NavigateUp => "IssuesNavigateUp",
    Self::NavigateDown => "IssuesNavigateDown",
    Self::NavigatePageUp(_) => "IssuesNavigatePageUp",
    Self::NavigatePageDown(_) => "IssuesNavigatePageDown",
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
    Self::ListSilentRefreshed { .. } => "IssueListSilentRefreshed",
    Self::ListSilentRefreshFailed { .. } => "IssueListSilentRefreshFailed",
    Self::DetailLoaded { .. } => "IssueDetailLoaded",
    Self::DetailLoadFailed { .. } => "IssueDetailLoadFailed",
    Self::DetailAuthRequired { .. } => "IssueDetailAuthRequired",
    Self::DetailSilentRefreshed { .. } => "IssueDetailSilentRefreshed",
    Self::DetailSilentRefreshFailed { .. } => "IssueDetailSilentRefreshFailed",
    Self::CommentsPageLoaded { .. } => "IssueCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "IssueCommentsPageFailed",
    Self::OpenFilterControls => "OpenFilterControls",
    Self::CloseFilterControls => "CloseFilterControls",
    Self::ApplyFilter => "ApplyFilter",
    Self::ClearFilter => "ClearFilter",
    Self::ClearDraftFilter => "ClearDraftFilter",
    Self::FilterNavigateNext => "FilterNavigateNext",
    Self::FilterNavigatePrev => "FilterNavigatePrev",
    Self::CycleFilterState => "CycleFilterState",
    Self::CycleIssueSortByNext => "CycleIssueSortByNext",
    Self::CycleIssueSortByPrev => "CycleIssueSortByPrev",
    Self::ToggleIssueSortOrder => "ToggleIssueSortOrder",
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
    Self::NewIssueTemplateNext => "NewIssueTemplateNext",
    Self::NewIssueTypeNext => "NewIssueTypeNext",
    Self::NewIssueTitleChar(_) => "NewIssueTitleChar",
    Self::NewIssueTitleBackspace => "NewIssueTitleBackspace",
    Self::NewIssueTitleDelete => "NewIssueTitleDelete",
    Self::NewIssueTitleCursorLeft => "NewIssueTitleCursorLeft",
    Self::NewIssueTitleCursorRight => "NewIssueTitleCursorRight",
    Self::NewIssueTitleCursorHome => "NewIssueTitleCursorHome",
    Self::NewIssueTitleCursorEnd => "NewIssueTitleCursorEnd",
    Self::NewIssueBodyChar(_) => "NewIssueBodyChar",
    Self::NewIssueBodyNewline => "NewIssueBodyNewline",
    Self::NewIssueBodyBackspace => "NewIssueBodyBackspace",
    Self::NewIssueBodyDelete => "NewIssueBodyDelete",
    Self::NewIssueBodyCursorLeft => "NewIssueBodyCursorLeft",
    Self::NewIssueBodyCursorRight => "NewIssueBodyCursorRight",
    Self::NewIssueBodyCursorUp => "NewIssueBodyCursorUp",
    Self::NewIssueBodyCursorDown => "NewIssueBodyCursorDown",
    Self::NewIssueBodyCursorHome => "NewIssueBodyCursorHome",
    Self::NewIssueBodyCursorEnd => "NewIssueBodyCursorEnd",
    Self::NewIssueFocusNext => "NewIssueFocusNext",
    Self::NewIssueFocusPrev => "NewIssueFocusPrev",
    Self::NewIssueSubmit => "NewIssueSubmit",
    Self::NewIssueCancel => "NewIssueCancel",
    Self::NewIssueOptionsLoaded { .. } => "NewIssueOptionsLoaded",
    Self::NewIssueOptionsFailed { .. } => "NewIssueOptionsFailed",
    Self::NewIssueCreated { .. } => "NewIssueCreated",
    Self::NewIssueCreateFailed { .. } => "NewIssueCreateFailed",
    Self::InlineChar(_) => "InlineChar",
    Self::InlineNewline => "InlineNewline",
    Self::InlineBackspace => "InlineBackspace",
    Self::InlineDelete => "InlineDelete",
    Self::InlineCursorLeft => "InlineCursorLeft",
    Self::InlineCursorRight => "InlineCursorRight",
    Self::InlineCursorUp => "InlineCursorUp",
    Self::InlineCursorDown => "InlineCursorDown",
    Self::InlineCursorHome => "InlineCursorHome",
    Self::InlineCursorEnd => "InlineCursorEnd",
    Self::InlineSubmit => "InlineSubmit",
    Self::InlineCancelOrEsc => "InlineCancelOrEsc",
    Self::RequestIssueRewrite => "RequestIssueRewrite",
    Self::IssueRewriteSucceeded { .. } => "IssueRewriteSucceeded",
    Self::IssueRewriteFailed { .. } => "IssueRewriteFailed",
    Self::MutationSubmitted { .. } => "MutationSubmitted",
    Self::IssueCreated { .. } => "IssueCreated",
    Self::CommentCreated { .. } => "CommentCreated",
    Self::CommentCreateFailed { .. } => "CommentCreateFailed",
    Self::IssueBodyUpdated { .. } => "IssueBodyUpdated",
    Self::CommentUpdated { .. } => "CommentUpdated",
    Self::MutationFailed { .. } => "MutationFailed",
    Self::CloseIssue => "CloseIssue",
    Self::OpenDeleteIssueConfirm => "OpenDeleteIssueConfirm",
    Self::IssueDeleteConfirm => "IssueDeleteConfirm",
    Self::IssueDeleteCancel => "IssueDeleteCancel",
    Self::OpenCloseReasonChooser => "OpenCloseReasonChooser",
    Self::CloseReasonNavigateUp => "CloseReasonNavigateUp",
    Self::CloseReasonNavigateDown => "CloseReasonNavigateDown",
    Self::CloseReasonSelect => "CloseReasonSelect",
    Self::CloseReasonDuplicateSearchChar(_) => "CloseReasonDuplicateSearchChar",
    Self::CloseReasonDuplicateSearchBackspace => "CloseReasonDuplicateSearchBackspace",
    Self::CloseReasonDuplicateSearchNavigateUp => "CloseReasonDuplicateSearchNavigateUp",
    Self::CloseReasonDuplicateSearchNavigateDown => "CloseReasonDuplicateSearchNavigateDown",
    Self::CloseReasonConfirm => "CloseReasonConfirm",
    Self::CloseReasonCancel => "CloseReasonCancel",
    Self::IssueClosed { .. } => "IssueClosed",
    Self::IssueDeleted { .. } => "IssueDeleted",
    Self::OpenAgentChooser { .. } => "OpenAgentChooser",
    Self::BeginListSendDetail { .. } => "BeginIssueListSendDetail",
    Self::CancelListSendDetail => "CancelIssueListSendDetail",
    Self::ListSendDetailReady { .. } => "IssueListSendDetailReady",
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
    Self::PropertyEditorTitleCursorHome => "IssuePropertyEditorTitleCursorHome",
    Self::PropertyEditorTitleCursorEnd => "IssuePropertyEditorTitleCursorEnd",
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
    Self::OpenChanges => "PrOpenChanges",
    Self::ChangesFocusContent => "PrChangesFocusContent",
    Self::ChangesFocusFiles => "PrChangesFocusFiles",
    Self::ChangesToggleView => "PrChangesToggleView",
    Self::OpenChangesComment => "PrOpenChangesComment",
    Self::ChangesBack => "PrChangesBack",
    Self::ChangesRetryFiles => "PrChangesRetryFiles",
    Self::ChangesRetryBlob => "PrChangesRetryBlob",
    Self::ChangesLoaded(_) => "PrChangesLoaded",
    Self::ChangesLoadFailed(_) => "PrChangesLoadFailed",
    Self::ChangesBlobLoaded(_) => "PrChangesBlobLoaded",
    Self::ChangesBlobLoadFailed(_) => "PrChangesBlobLoadFailed",
    Self::ListLoaded { .. } => "PrListLoaded",
    Self::ListLoadFailed { .. } => "PrListLoadFailed",
    Self::ListPageLoaded { .. } => "PrListPageLoaded",
    Self::ListSilentRefreshed { .. } => "PrListSilentRefreshed",
    Self::ListSilentRefreshFailed { .. } => "PrListSilentRefreshFailed",
    Self::DetailLoaded { .. } => "PrDetailLoaded",
    Self::DetailLoadFailed { .. } => "PrDetailLoadFailed",
    Self::DetailAuthRequired { .. } => "PrDetailAuthRequired",
    Self::DetailSilentRefreshed { .. } => "PrDetailSilentRefreshed",
    Self::DetailSilentRefreshFailed { .. } => "PrDetailSilentRefreshFailed",
    Self::CommentsPageLoaded { .. } => "PrCommentsPageLoaded",
    Self::CommentsPageFailed { .. } => "PrCommentsPageFailed",
    Self::CommentsPageDispatchFailed { .. } => "PrCommentsPageDispatchFailed",
    Self::OpenFilterControls => "PrOpenFilterControls",
    Self::CloseFilterControls => "PrCloseFilterControls",
    Self::ApplyFilter => "PrApplyFilter",
    Self::ClearFilter => "PrClearFilter",
    Self::FilterNavigate(_) => "PrFilterNavigate",
    Self::CycleFilterState => "PrCycleFilterState",
    Self::CycleDraftFilter => "PrCycleDraftFilter",
    Self::CycleReviewFilter => "PrCycleReviewFilter",
    Self::CycleChecksFilter => "PrCycleChecksFilter",
    Self::PrCycleSortByNext => "PrCycleSortByNext",
    Self::PrCycleSortByPrev => "PrCycleSortByPrev",
    Self::PrToggleSortOrder => "PrToggleSortOrder",
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
    Self::BeginListSendDetail { .. } => "BeginPrListSendDetail",
    Self::CancelListSendDetail => "CancelPrListSendDetail",
    Self::ListSendDetailReady { .. } => "PrListSendDetailReady",
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
    Self::MergeMethodsLoadFailed { .. } => "PrMergeMethodsLoadFailed",
    Self::OpenDeleteConfirm => "PrOpenDeleteConfirm",
    Self::DeleteConfirm => "PrDeleteConfirm",
    Self::DeleteCancel => "PrDeleteCancel",
    Self::Deleted { .. } => "PrDeleted",
    Self::DeleteFailed { .. } => "PrDeleteFailed",
    Self::OpenNewForm => "PrOpenNewForm",
    Self::NewFormCancel => "PrNewFormCancel",
    Self::NewFormFocusNext => "PrNewFormFocusNext",
    Self::NewFormFocusPrevious => "PrNewFormFocusPrevious",
    Self::NewFormBranchUp => "PrNewFormBranchUp",
    Self::NewFormBranchDown => "PrNewFormBranchDown",
    Self::NewFormChar(_) => "PrNewFormChar",
    Self::NewFormNewline => "PrNewFormNewline",
    Self::NewFormBackspace => "PrNewFormBackspace",
    Self::NewFormDelete => "PrNewFormDelete",
    Self::NewFormCursorLeft => "PrNewFormCursorLeft",
    Self::NewFormCursorRight => "PrNewFormCursorRight",
    Self::NewFormCursorHome => "PrNewFormCursorHome",
    Self::NewFormCursorEnd => "PrNewFormCursorEnd",
    Self::NewFormSubmit => "PrNewFormSubmit",
    Self::BranchesLoaded { .. } => "PrBranchesLoaded",
    Self::BranchesFailed { .. } => "PrBranchesFailed",
    Self::Created { .. } => "PrCreated",
    Self::CreateFailed { .. } => "PrCreateFailed",
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
    Self::PropertyEditorTitleCursorHome => "PrPropertyEditorTitleCursorHome",
    Self::PropertyEditorTitleCursorEnd => "PrPropertyEditorTitleCursorEnd",
    Self::PropertyEditorOptionsLoaded { .. } => "PrPropertyEditorOptionsLoaded",
    Self::PropertyEditorOptionsFailed { .. } => "PrPropertyEditorOptionsFailed",
    Self::PropertyEditSucceeded { .. } => "PrPropertyEditSucceeded",
    Self::PostMutationRefreshStarted => "PrPostMutationRefreshStarted",
    Self::PropertyEditFailed { .. } => "PrPropertyEditFailed",
    Self::PropertyEditorValidationError { .. } => "PrPropertyEditorValidationError",
});

use crate::state::AppEvent;

#[must_use]
pub(super) fn is_new_issue_form_app_event(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::NewIssueTemplateNext
            | AppEvent::NewIssueTypeNext
            | AppEvent::NewIssueTitleChar(_)
            | AppEvent::NewIssueTitleBackspace
            | AppEvent::NewIssueTitleDelete
            | AppEvent::NewIssueTitleCursorLeft
            | AppEvent::NewIssueTitleCursorRight
            | AppEvent::NewIssueTitleCursorHome
            | AppEvent::NewIssueTitleCursorEnd
            | AppEvent::NewIssueBodyChar(_)
            | AppEvent::NewIssueBodyNewline
            | AppEvent::NewIssueBodyBackspace
            | AppEvent::NewIssueBodyDelete
            | AppEvent::NewIssueBodyCursorLeft
            | AppEvent::NewIssueBodyCursorRight
            | AppEvent::NewIssueBodyCursorUp
            | AppEvent::NewIssueBodyCursorDown
            | AppEvent::NewIssueBodyCursorHome
            | AppEvent::NewIssueBodyCursorEnd
            | AppEvent::NewIssueFocusNext
            | AppEvent::NewIssueFocusPrev
            | AppEvent::NewIssueSubmit
            | AppEvent::NewIssueCancel
            | AppEvent::NewIssueOptionsLoaded { .. }
            | AppEvent::NewIssueOptionsFailed { .. }
            | AppEvent::NewIssueCreated { .. }
            | AppEvent::NewIssueCreateFailed { .. }
    )
}

#[must_use]
pub fn is_new_issue_form_msg(message: &IssuesMessage) -> bool {
    matches!(
        message,
        IssuesMessage::NewIssueTemplateNext
            | IssuesMessage::NewIssueTypeNext
            | IssuesMessage::NewIssueTitleChar(_)
            | IssuesMessage::NewIssueTitleBackspace
            | IssuesMessage::NewIssueTitleDelete
            | IssuesMessage::NewIssueTitleCursorLeft
            | IssuesMessage::NewIssueTitleCursorRight
            | IssuesMessage::NewIssueTitleCursorHome
            | IssuesMessage::NewIssueTitleCursorEnd
            | IssuesMessage::NewIssueBodyChar(_)
            | IssuesMessage::NewIssueBodyNewline
            | IssuesMessage::NewIssueBodyBackspace
            | IssuesMessage::NewIssueBodyDelete
            | IssuesMessage::NewIssueBodyCursorLeft
            | IssuesMessage::NewIssueBodyCursorRight
            | IssuesMessage::NewIssueBodyCursorUp
            | IssuesMessage::NewIssueBodyCursorDown
            | IssuesMessage::NewIssueBodyCursorHome
            | IssuesMessage::NewIssueBodyCursorEnd
            | IssuesMessage::NewIssueFocusNext
            | IssuesMessage::NewIssueFocusPrev
            | IssuesMessage::NewIssueSubmit
            | IssuesMessage::NewIssueCancel
            | IssuesMessage::NewIssueOptionsLoaded { .. }
            | IssuesMessage::NewIssueOptionsFailed { .. }
            | IssuesMessage::NewIssueCreated { .. }
            | IssuesMessage::NewIssueCreateFailed { .. }
    )
}

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
            | AppEvent::IssuePropertyEditorTitleCursorHome
            | AppEvent::IssuePropertyEditorTitleCursorEnd
            | AppEvent::IssuePropertyEditorOptionsLoaded { .. }
            | AppEvent::IssuePropertyEditorOptionsFailed { .. }
            | AppEvent::IssuePropertyEditSucceeded { .. }
            | AppEvent::IssuePostMutationRefreshStarted
            | AppEvent::IssuePropertyEditFailed { .. }
            | AppEvent::IssuePropertyEditorValidationError { .. }
    )
}

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
            | IssuesMessage::PropertyEditorTitleCursorHome
            | IssuesMessage::PropertyEditorTitleCursorEnd
            | IssuesMessage::PropertyEditorOptionsLoaded { .. }
            | IssuesMessage::PropertyEditorOptionsFailed { .. }
            | IssuesMessage::PropertyEditSucceeded { .. }
            | IssuesMessage::PostMutationRefreshStarted
            | IssuesMessage::PropertyEditFailed { .. }
            | IssuesMessage::PropertyEditorValidationError { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::{is_new_issue_form_app_event, is_new_issue_form_msg};
    use crate::messages::IssuesMessage;
    use crate::state::AppEvent;

    #[test]
    fn app_event_predicates_match_all_new_issue_form_variants() {
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueSubmit));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueCancel));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueTemplateNext));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueTypeNext));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueFocusNext));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueFocusPrev));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueTitleChar(
            'x'
        )));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueBodyChar(
            'y'
        )));
        assert!(is_new_issue_form_app_event(&AppEvent::NewIssueBodyNewline));
    }

    #[test]
    fn app_event_predicates_reject_unrelated_events() {
        assert!(!is_new_issue_form_app_event(&AppEvent::EnterIssuesMode));
        assert!(!is_new_issue_form_app_event(
            &AppEvent::OpenNewIssueComposer
        ));
        assert!(!is_new_issue_form_app_event(&AppEvent::IssuesNavigateUp));
    }

    #[test]
    fn msg_predicates_match_all_new_issue_form_variants() {
        assert!(is_new_issue_form_msg(&IssuesMessage::NewIssueSubmit));
        assert!(is_new_issue_form_msg(&IssuesMessage::NewIssueCancel));
        assert!(is_new_issue_form_msg(&IssuesMessage::NewIssueTemplateNext));
    }

    #[test]
    fn msg_predicates_reject_unrelated_messages() {
        assert!(!is_new_issue_form_msg(&IssuesMessage::EnterMode));
    }
}
