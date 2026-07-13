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

use crate::domain::RepositoryId;

use super::{AgentChooserState, ComposerTarget, InlineState, PriorAgentFocus};

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
    /// `C` pressed on an already-closed issue (issue #182).
    IssueAlreadyClosed,
    /// `C`/`D` pressed with no issue focused (issue #182).
    NoIssueFocused,
}

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
    pub loading: PrLoadingState,
    pub error: Option<String>,
    pub pr_focus: PrFocus,
    pub detail_subfocus: PrDetailSubfocus,
    /// First-visible PR-list row; driven by the shared selection-follow helper.
    pub list_scroll_offset: usize,
    /// PR-list pane height in rows (set as a prop from the layout module).
    pub list_viewport_rows: usize,
    /// Scroll offset (in lines) for the detail pane viewport.
    pub detail_scroll_offset: usize,
    /// Last rendered detail viewport height in rows.
    pub detail_viewport_rows: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    /// Merge-method chooser overlay state (issue #92; mirrors AgentChooser).
    pub merge_chooser: Option<PrMergeChooserState>,
    /// Pending merge mutation staleness guard (issue #92).
    pub merge_mutation_pending: Option<PrMergeMutationPending>,
    pub filter_ui: PrFilterUiState,
    pub search_input_focused: bool,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub draft_notice: Option<String>,
    pub mutation_pending: Option<PrMutationPending>,
    pub next_mutation_id: u64,
    pub detail_pending: Option<PrDetailPending>,
    pub next_pr_detail_request_id: u64,
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

/// Pending review-thread resolve/unresolve mutation staleness guard
/// (issue #119). Tracks the in-flight thread resolve toggle so the UI can
/// show a pending state and ignore stale responses.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrThreadResolvePending {
    pub scope_repo_id: RepositoryId,
    pub thread_index: usize,
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
/// PR filter UI state.
/// `field_index` ranges over the EIGHT filter fields:
/// 0 state, 1 draft, 2 review-decision, 3 checks-status,
/// 4 author, 5 assignee, 6 reviewer, 7 labels.
#[derive(Debug, Clone, Default)]
pub struct PrFilterUiState {
    pub controls_open: bool,
    pub field_index: usize,
    pub draft_labels_text: String,
}
