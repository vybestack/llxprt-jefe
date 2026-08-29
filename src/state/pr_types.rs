//! Pull Requests Mode state types (extracted from types.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P03
//! @requirement REQ-PR-001
//! @requirement REQ-PR-003
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @requirement REQ-PR-008
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013

use crate::domain::{ListRequestId, RepositoryId};

use super::{AgentChooserState, ComposerTarget, InlineState};

/// Identity for the PRs list — a result is stale unless both the scope repo
/// and the committed filter match exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListIdentity {
    /// Repository scope the list was loaded for.
    pub scope_repo_id: RepositoryId,
    /// Committed filter snapshot when the load was started.
    pub filter: crate::domain::PrFilter,
}

// =============================================================================
// Pull Requests Mode state types
//
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-001
// @requirement REQ-PR-003
// @requirement REQ-PR-006
// @requirement REQ-PR-008
// @requirement REQ-PR-009
// @requirement REQ-PR-010
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-76
/// Focus domain within PR Mode — separate from PaneFocus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrFocus {
    RepoList,
    #[default]
    PrList,
    PrDetail,
    /// Optional changed-files review drill-down for the loaded PR.
    PrChanges,
}

/// Focus area inside the Changes drill-down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrChangesFocus {
    /// Changed-files list.
    #[default]
    FileList,
    /// Selected file content.
    Content,
}

/// Content mode for the selected changed file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrDiffViewMode {
    /// Unified diff hunks returned by GitHub.
    #[default]
    DeltasOnly,
    /// Full immutable file blob with diff rows interleaved.
    FullFile,
}

/// Stable identity of one Changes visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesIdentity {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub head_sha: String,
}

/// Pending changed-files read correlation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesPending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub head_sha: String,
    pub request_id: u64,
}

/// Pending immutable blob read correlation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesBlobPending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub blob_sha: String,
}

/// One immutable blob cached for the current Changes visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesBlobCache {
    pub blob_sha: String,
    pub blob: crate::domain::PrFileBlob,
}

/// Successful changed-files load correlated to one PR visit.
///
/// Carries the expected head SHA so the reducer can reject completions that
/// arrive after a refresh moved the PR to a different head (issue #376).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesLoadedPayload {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub head_sha: String,
    pub files: Vec<crate::domain::PrFileChange>,
    pub truncated: bool,
}

/// Failed changed-files load correlated to one PR visit.
///
/// Carries the expected head SHA so the reducer can reject stale failures the
/// same way it rejects stale successes (issue #376).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesLoadFailedPayload {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub head_sha: String,
    pub error: String,
}

/// Successful immutable-blob load correlated to one PR visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesBlobLoadedPayload {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub blob_sha: String,
    pub blob: crate::domain::PrFileBlob,
}

/// Failed immutable-blob load correlated to one PR visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrChangesBlobLoadFailedPayload {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub blob_sha: String,
    pub error: String,
}

/// Transient state for the optional changed-files review drill-down.
#[derive(Debug, Clone, Default)]
pub struct PrChangesState {
    pub identity: Option<PrChangesIdentity>,
    pub pending: Option<PrChangesPending>,
    pub blob_pending: Option<PrChangesBlobPending>,
    /// The last blob request_id that the dispatch layer spawned a task for,
    /// so repeated navigation events for the same pending request do not spawn
    /// duplicate tasks (issue #376 edge-triggered dispatch).
    pub blob_dispatched_request_id: Option<u64>,
    pub blobs: Vec<PrChangesBlobCache>,
    pub blob_error: Option<String>,
    pub files: Vec<crate::domain::PrFileChange>,
    pub selected_file: Option<usize>,
    pub focus: PrChangesFocus,
    pub view_mode: PrDiffViewMode,
    pub selected_row: Option<usize>,
    pub truncated: bool,
    pub error: Option<String>,
    pub next_request_id: u64,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 201-207
/// Subfocus within PR detail view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrDetailSubfocus {
    #[default]
    Body,
    Review(usize),
    /// Focus on a review thread (flat index across all reviews' threads).
    ReviewThread(usize),
    Check(usize),
    Comment(usize),
    NewComment,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-010
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-003 lines 83-89
///
/// Canonical read-only hint kind for invalid `r`/`c`/`e`/`o`/`m` actions.
/// Carried by `AppEvent::PrShowNotice` to surface a non-blocking hint
/// instead of silently dropping the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnlyHintKind {
    /// `r` pressed on body/review/check/new-comment (reply only valid on a comment).
    ReadOnlyReplyOnComment,
    /// `c` pressed on a review/check item (reviews and checks are read-only).
    ReadOnlyNoComment,
    /// `e` pressed anywhere in PR detail (body/reviews/checks not editable in v1).
    ReadOnlyNotEditable,
    /// `o` pressed with no PR selected/loaded (nothing to open in browser).
    NoSelectionToOpen,
    /// `m` pressed with no loaded PR detail (nothing to merge).
    NoPrToMerge,
    /// `m` pressed on a PR that is not in an open+mergeable state.
    PrNotMergeable,
    /// `R` pressed outside a review thread (resolve only valid on a review thread).
    ReadOnlyResolveOnThread,
    /// A delete was requested with no pull request focused (issue #183).
    NoPrToDelete,
    /// `C` pressed on an already-closed issue (issue #182).
    IssueAlreadyClosed,
    /// `C`/`D` pressed with no issue focused (issue #182).
    NoIssueFocused,
    /// Duplicate close attempted with no duplicate target selected (issue #188).
    NoDuplicateTarget,
}

impl ReadOnlyHintKind {
    #[must_use]
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::ReadOnlyReplyOnComment => "Select a comment to reply (read-only context)",
            Self::ReadOnlyNoComment => "No comments to reply to",
            Self::ReadOnlyNotEditable => "This section is read-only",
            Self::NoSelectionToOpen => "No pull request selected to open",
            Self::NoPrToMerge => "No pull request loaded to merge",
            Self::PrNotMergeable => "Pull request is not mergeable (closed/merged)",
            Self::ReadOnlyResolveOnThread => {
                "Select a review thread to resolve (read-only context)"
            }
            Self::NoPrToDelete => "No pull request selected to delete",
            Self::IssueAlreadyClosed => "Issue is already closed",
            Self::NoIssueFocused => "No issue selected",
            Self::NoDuplicateTarget => "Select an issue to mark as duplicate",
        }
    }
}

pub const NO_AGENTS_AVAILABLE: &str = "No agents available";

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 62-65
/// Aggregate state for PR Mode (mirrors `IssuesState`).
#[derive(Debug, Clone, Default)]
pub struct PullRequestsState {
    pub active: bool,
    /// Unified list state: PRs, selection, pagination continuation, and
    /// pending load correlation. List loading is derived from this container.
    pub list: crate::state::pagination::PaginatedList<crate::domain::PullRequest, PrListIdentity>,
    pub pr_detail: Option<crate::domain::PullRequestDetail>,
    pub committed_filter: crate::domain::PrFilter,
    pub draft_filter: crate::domain::PrFilter,
    pub search_query: String,
    /// Active list sort (issue #473). Projection-time view transform; lives
    /// on the state (not the fetch-time identity) so toggling it never re-runs
    /// the fetch or perturbs stale-rejection.
    pub sort_config: crate::domain::PrSortConfig,
    pub loading: PrLoadingState,
    pub error: Option<String>,
    pub pr_focus: PrFocus,
    /// Transient optional changed-files review state.
    pub changes: PrChangesState,
    pub detail_subfocus: PrDetailSubfocus,
    /// Scroll offset (in lines) for the detail pane viewport.
    pub detail_scroll_offset: usize,
    /// Last rendered detail viewport height in rows.
    pub detail_viewport_rows: usize,
    /// Last rendered detail content width in terminal cells.
    pub detail_content_width: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    /// Merge-method chooser overlay state (issue #92; mirrors AgentChooser).
    pub merge_chooser: Option<PrMergeChooserState>,
    /// Pending merge mutation staleness guard (issue #92).
    pub merge_mutation_pending: Option<PrMergeMutationPending>,
    /// Destructive-confirm overlay for deleting a pull request (issue #183).
    pub delete_confirm: Option<PrDeleteConfirmState>,
    /// In-flight pull-request delete (issue #183).
    pub delete_mutation_pending: Option<PrDeleteMutationPending>,
    /// New PR composer draft (issue #183).
    pub new_pr_form: Option<NewPrFormState>,
    /// In-flight pull-request creation (issue #183).
    pub create_mutation_pending: Option<PrCreateMutationPending>,
    pub filter_ui: PrFilterUiState,
    pub search_input_focused: bool,
    pub draft_notice: Option<String>,
    pub mutation_pending: Option<PrMutationPending>,
    pub next_mutation_id: u64,
    /// Property editor overlay state (issue #175).
    pub property_editor: Option<super::PrPropertyEditorState>,
    /// Pending property mutation staleness guard (issue #175).
    pub property_mutation_pending: Option<super::PropertyMutationPending>,
    /// Coalesced silent refresh requested by a successful property mutation.
    pub post_mutation_refresh: crate::state::post_mutation_refresh::PostMutationRefresh,
    /// Monotonic request id for property option loads / mutations (issue #175).
    pub next_property_request_id: u64,
    pub detail_pending: Option<PrDetailPending>,
    pub list_send_pending: Option<PrListSendPending>,
    pub next_pr_detail_request_id: u64,
    /// High-water mark retained across replaceable PR-detail snapshots.
    pub last_comments_page_request_id: ListRequestId,
    /// Pending review-thread resolve/unresolve mutation (issue #119).
    pub thread_resolve_pending: Option<PrThreadResolvePending>,
    /// Monotonic request id for thread-resolve mutations (issue #119).
    pub next_thread_resolve_request_id: u64,
}

impl PullRequestsState {
    /// Read-only access to the loaded pull requests.
    #[must_use]
    pub fn pull_requests(&self) -> &[crate::domain::PullRequest] {
        self.list.items()
    }

    /// The currently selected PR index, if any.
    #[must_use]
    pub fn selected_pr_index(&self) -> Option<usize> {
        self.list.selected_index()
    }

    /// Whether the list is visibly loading (reload-visible or page pending).
    #[must_use]
    pub fn list_loading(&self) -> bool {
        self.list.is_loading()
    }

    /// Whether any list operation is pending (visible or silent).
    #[must_use]
    pub fn list_pending(&self) -> bool {
        self.list.has_pending_request()
    }

    /// Whether more pages are available.
    #[must_use]
    pub fn has_more_prs(&self) -> bool {
        self.list.has_more()
    }
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 88-98
/// Pending detail-load staleness guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDetailPending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListSendPending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub request_id: u64,
    pub metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    pub ready: bool,
}

/// Pending review-thread resolve/unresolve mutation staleness guard
/// (issue #119). Tracks the in-flight thread resolve toggle so the UI can
/// show a pending state and ignore stale responses.
///
/// Carries `pr_number` so the reducer can reject a completion that arrives
/// after a PR/repository/mode identity change moved focus to a different PR
/// (issue #376).
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrThreadResolvePending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub thread_index: usize,
    /// Stable thread node id captured at dispatch time so the write-back can
    /// locate the correct thread even if a background refresh reorders
    /// `detail.reviews` while the mutation is in flight (issue #238).
    pub thread_id: String,
    pub resolve: bool,
    pub request_id: u64,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 88-98
/// Pending comment-create mutation staleness guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrMutationPending {
    pub scope_repo_id: RepositoryId,
    pub mutation_id: u64,
    pub target: ComposerTarget,
}

/// Destructive-confirm overlay for deleting a pull request (issue #183).
///
/// Mirrors `IssueDeleteConfirmState`: the overlay opens unarmed, the first
/// confirmation arms it, and the second dispatches. It carries the branch names
/// resolved when it opened so the confirmation cannot act on a different pull
/// request than the one the user read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDeleteConfirmState {
    pub pr_number: u64,
    /// The branch that would be removed.
    pub head_ref: String,
    /// The branch the pull request targets, which must never be removed.
    pub base_ref: String,
    /// Whether the pull request is still open, and so must be closed first.
    pub is_open: bool,
    pub awaiting_confirmation: bool,
}

/// In-flight pull-request delete (issue #183).
///
/// Mirrors `PrMergeMutationPending`. The full identity is carried so a result
/// that arrives after the selection moved cannot retire the wrong operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDeleteMutationPending {
    pub scope_repo_id: RepositoryId,
    pub mutation_id: u64,
    pub pr_number: u64,
    pub head_ref: String,
    /// Whether the pull request must be closed before its branch is removed.
    pub close_first: bool,
}

/// Which field the New PR composer is editing (issue #183).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NewPrFormFocus {
    /// The branch the pull request is opened from.
    #[default]
    Head,
    /// The branch the pull request is opened against.
    Base,
    Title,
    Body,
}

impl NewPrFormFocus {
    /// The next field, wrapping at the end.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Head => Self::Base,
            Self::Base => Self::Title,
            Self::Title => Self::Body,
            Self::Body => Self::Head,
        }
    }

    /// The previous field, wrapping at the start.
    #[must_use]
    pub const fn previous(self) -> Self {
        match self {
            Self::Head => Self::Body,
            Self::Base => Self::Head,
            Self::Title => Self::Base,
            Self::Body => Self::Title,
        }
    }
}

/// Draft state for the New PR composer (issue #183).
///
/// Mirrors `NewIssueFormState`: pure reducer state with no I/O. The boundary
/// layer reads it on submit and drives the create call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewPrFormState {
    /// The repository's branches, alphabetically ascending. Empty while the
    /// background load is in flight.
    pub branches: Vec<String>,
    /// Index into `branches` for the head branch.
    pub head_index: usize,
    /// Index into `branches` for the base branch.
    pub base_index: usize,
    pub title_text: String,
    pub title_cursor: usize,
    pub body_text: String,
    pub body_cursor: usize,
    pub focus: NewPrFormFocus,
    /// Footer error: a load failure or a refused submit. Blankable.
    pub error: Option<String>,
    /// Whether the branch load is still in flight. Submit is blocked while set.
    pub branches_loading: bool,
    /// Correlates the branch load so a stale answer cannot fill this composer.
    pub load_request_id: u64,
}

impl NewPrFormState {
    /// The selected head branch, if the branch list has arrived.
    #[must_use]
    pub fn head_branch(&self) -> Option<&str> {
        self.branches.get(self.head_index).map(String::as_str)
    }

    /// The selected base branch, if the branch list has arrived.
    #[must_use]
    pub fn base_branch(&self) -> Option<&str> {
        self.branches.get(self.base_index).map(String::as_str)
    }
}

/// In-flight pull-request creation (issue #183).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrCreateMutationPending {
    pub scope_repo_id: RepositoryId,
    pub mutation_id: u64,
}

/// Merge-method chooser overlay state (issue #92; mirrors AgentChooserState).
///
/// `selected_index` ranges over [`crate::domain::MERGE_METHODS`].
/// `allowed_methods` is `None` until the repo settings fetch resolves; while
/// `None`, ALL methods are shown as available. Once loaded, methods NOT in
/// the list are rendered disabled.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone)]
pub struct PrMergeChooserState {
    /// 0-based index into [`crate::domain::MERGE_METHODS`].
    pub selected_index: usize,
    /// Methods allowed by repo settings; `None` until fetched.
    pub allowed_methods: Option<Vec<crate::domain::MergeMethod>>,
    /// True when the confirmation step is active (second Enter triggers merge).
    pub awaiting_confirmation: bool,
}

/// Pending merge mutation staleness guard (issue #92; mirrors PrMutationPending).
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrMergeMutationPending {
    pub scope_repo_id: RepositoryId,
    pub mutation_id: u64,
    pub pr_number: u64,
    pub method: crate::domain::MergeMethod,
}

/// Loading/pending state for PR mode async operations.
///
/// List loading is now derived from `PullRequestsState::list` (the
/// `PaginatedList::is_loading()` / `has_pending_request()` accessors). Only
/// detail and comments loading remain as explicit flags here.
#[derive(Debug, Clone, Default)]
pub struct PrLoadingState {
    pub detail: bool,
    pub comments: bool,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
///
/// PR filter UI state.
/// `field_index` ranges over the TEN filter+sort fields:
/// 0 state, 1 draft, 2 review-decision, 3 checks-status,
/// 4 author, 5 assignee, 6 reviewer, 7 labels,
/// 8 sort_by, 9 sort_order (issue #473).
#[derive(Debug, Clone, Default)]
pub struct PrFilterUiState {
    pub controls_open: bool,
    pub field_index: usize,
    pub draft_labels_text: String,
}

/// Index of the sort-by field within the PR filter dialog (issue #473).
pub const PR_SORT_BY_FIELD_INDEX: usize = 8;
/// Index of the sort-order field within the PR filter dialog (issue #473).
pub const PR_SORT_ORDER_FIELD_INDEX: usize = 9;
