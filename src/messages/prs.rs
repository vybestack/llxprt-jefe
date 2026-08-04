//! Pull-request mode messages.
//!
//! Split out of `messages.rs` to keep that file within the source-size gate,
//! following the same pattern as the actions, errors, and terminal-manager
//! message modules.

use super::{
    IssueComment, MergeMethod, NavDir, PrFilter, PrFilterField, PrInlineMsg, PullRequest,
    PullRequestDetail, ReadOnlyHintKind, RepositoryId, ScrollDir,
};

// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-002
#[derive(Debug, Clone)]
pub enum PullRequestsMessage {
    EnterMode,
    ExitMode,
    RefocusList,
    Navigate(NavDir),
    Enter,
    CycleFocus,
    CycleFocusReverse,
    ScrollDetail(ScrollDir),
    DetailSubfocusNext,
    DetailSubfocusPrev,
    OpenChanges,
    ChangesFocusContent,
    ChangesFocusFiles,
    ChangesToggleView,
    /// Open a line-review composer for the selected Changes row.
    OpenChangesComment,
    ChangesBack,
    /// Retry the changed-files read after a terminal failure (issue #376).
    ChangesRetryFiles,
    /// Retry the selected full-file blob read after a terminal failure
    /// (issue #376).
    ChangesRetryBlob,
    ChangesLoaded(crate::state::PrChangesLoadedPayload),
    ChangesLoadFailed(crate::state::PrChangesLoadFailedPayload),
    ChangesBlobLoaded(crate::state::PrChangesBlobLoadedPayload),
    ChangesBlobLoadFailed(crate::state::PrChangesBlobLoadFailedPayload),
    ListLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<PrFilter>,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    ListLoadFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    ListPageLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background refresh succeeded (issue #128).
    ListSilentRefreshed {
        scope_repo_id: RepositoryId,
        filter: Box<PrFilter>,
        request_id: u64,
        pull_requests: Vec<PullRequest>,
        cursor: Option<String>,
        has_more: bool,
    },
    /// Silent background refresh failed (issue #128).
    ListSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    DetailLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<PullRequestDetail>,
    },
    DetailLoadFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },
    DetailAuthRequired {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    },
    /// Silent background detail refresh succeeded (issue #128).
    DetailSilentRefreshed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        detail: Box<PullRequestDetail>,
    },
    /// Silent background detail refresh failed (issue #128).
    DetailSilentRefreshFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    },
    CommentsPageLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        comments: Vec<IssueComment>,
        cursor: Option<String>,
        has_more: bool,
    },
    CommentsPageFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
        error: String,
    },
    CommentsPageDispatchFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
    OpenFilterControls,
    CloseFilterControls,
    ApplyFilter,
    ClearFilter,
    FilterNavigate(NavDir),
    CycleFilterState,
    CycleDraftFilter,
    CycleReviewFilter,
    CycleChecksFilter,
    PrCycleSortByNext,
    PrCycleSortByPrev,
    PrToggleSortOrder,
    UpdateDraftFilter {
        field: PrFilterField,
        value: String,
    },
    FocusSearchInput,
    BlurSearchInput,
    SetSearchQuery {
        query: String,
    },
    ApplySearch,
    ClearSearch,
    OpenNewCommentComposer,
    OpenReplyComposer {
        comment_index: usize,
    },
    Inline(PrInlineMsg),
    CommentCreated {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        comment: IssueComment,
    },
    CommentCreateFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    MutationFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    ShowNotice(ReadOnlyHintKind),
    OpenAgentChooser {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    BeginListSendDetail {
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    },
    CancelListSendDetail,
    ListSendDetailReady {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        request_id: u64,
    },
    AgentChooserNavigate(NavDir),
    AgentChooserConfirm,
    AgentChooserCancel,
    SendToAgentCompleted,
    SendToAgentFailed {
        error: String,
    },
    OpenInBrowser,
    OpenedInBrowser {
        scope_repo_id: RepositoryId,
        pr_number: u64,
    },
    OpenInBrowserFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
    // PR In-App Merge (issue #92)
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-009
    OpenMergeChooser,
    MergeNavigate(NavDir),
    MergeConfirm,
    MergeCancel,
    Merged {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        method: MergeMethod,
    },
    MergeFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    MergeMethodsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        allowed_methods: Vec<MergeMethod>,
    },
    MergeMethodsLoadFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
    // PR Review Threads (issue #119)
    /// Open the inline reply composer for a review thread.
    OpenThreadReply {
        thread_index: usize,
    },
    /// Toggle resolve/unresolve on a focused review thread.
    ToggleThreadResolve {
        thread_index: usize,
    },
    /// A review-thread resolve/unresolve mutation succeeded.
    ThreadResolveSucceeded {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        is_resolved: bool,
        request_id: u64,
    },
    /// A review-thread resolve/unresolve mutation failed.
    ThreadResolveFailed {
        scope_repo_id: RepositoryId,
        thread_index: usize,
        request_id: u64,
        error: String,
    },
    // Property editing (issue #175)
    OpenPropertyEditor {
        kind: crate::state::PrPropertyKind,
    },
    PropertyEditorNavigateUp,
    PropertyEditorNavigateDown,
    PropertyEditorToggle,
    PropertyEditorConfirm,
    PropertyEditorCancel,
    PropertyEditorTitleChar(char),
    PropertyEditorTitleBackspace,
    PropertyEditorTitleDelete,
    PropertyEditorTitleCursorLeft,
    PropertyEditorTitleCursorRight,
    PropertyEditorTitleCursorHome,
    PropertyEditorTitleCursorEnd,
    PropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        options: Vec<(Option<String>, String, bool)>,
    },
    PropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    PropertyEditSucceeded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
    },
    /// Consume a queued PR refresh immediately before orchestration starts it.
    PostMutationRefreshStarted,
    PropertyEditFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        kind: crate::state::PrPropertyKind,
        request_id: u64,
        error: String,
    },
    /// Synchronous validation error set directly on the open editor (issue #175).
    PropertyEditorValidationError {
        kind: crate::state::PrPropertyKind,
        error: String,
    },
}
