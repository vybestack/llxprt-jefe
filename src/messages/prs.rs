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
/// Closed vertical direction for the merge-method chooser.
///
/// The chooser is a short fixed list with no paging, so only `Up` and `Down`
/// are representable; the closed payload makes totalizing other
/// [`NavDir`](super::NavDir) variants unrepresentable at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeNavDirection {
    Up,
    Down,
}

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
    MergeNavigate(MergeNavDirection),
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
    // PR delete: close plus head-branch removal (issue #183)
    /// Open the destructive confirm overlay for the focused pull request.
    OpenDeleteConfirm,
    /// Arm the delete overlay, or dispatch the delete once armed.
    DeleteConfirm,
    /// Close the delete overlay without deleting.
    DeleteCancel,
    /// The head branch was removed, and the pull request closed when `closed`.
    Deleted {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        branch: String,
        closed: bool,
    },
    /// The close or the branch removal failed. `closed` reports whether the
    /// close had already succeeded.
    DeleteFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        closed: bool,
        error: String,
    },
    // New PR composer (issue #183)
    /// Open the New PR composer.
    OpenNewForm,
    /// Close the composer and discard the draft.
    NewFormCancel,
    /// Move to the next composer field.
    NewFormFocusNext,
    /// Move to the previous composer field.
    NewFormFocusPrevious,
    /// Move the focused branch selection towards the start of the list.
    NewFormBranchUp,
    /// Move the focused branch selection towards the end of the list.
    NewFormBranchDown,
    /// Type into the focused text field.
    NewFormChar(char),
    /// Break the body onto a new line.
    NewFormNewline,
    /// Delete the character before the cursor.
    NewFormBackspace,
    /// Delete the character at the cursor.
    NewFormDelete,
    /// Move the cursor one character towards the start.
    NewFormCursorLeft,
    /// Move the cursor one character towards the end.
    NewFormCursorRight,
    /// Move the cursor to the start of the field.
    NewFormCursorHome,
    /// Move the cursor to the end of the field.
    NewFormCursorEnd,
    /// Open the pull request the composer describes.
    NewFormSubmit,
    /// The repository's branches arrived for an open composer.
    BranchesLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        branches: Vec<String>,
        default_branch: Option<String>,
    },
    /// The repository's branches could not be listed.
    BranchesFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    /// A pull request was opened.
    Created {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        pr_number: u64,
    },
    /// A pull request could not be opened.
    CreateFailed {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
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
