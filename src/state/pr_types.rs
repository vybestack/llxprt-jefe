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
/// Canonical read-only hint kind for invalid `r`/`c`/`e`/`o` actions.
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
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 62-65
/// Aggregate state for PR Mode (mirrors `IssuesState`).
#[derive(Debug, Clone, Default)]
pub struct PullRequestsState {
    pub active: bool,
    pub pull_requests: Vec<crate::domain::PullRequest>,
    pub selected_pr_index: Option<usize>,
    pub pr_detail: Option<crate::domain::PullRequestDetail>,
    pub committed_filter: crate::domain::PrFilter,
    pub draft_filter: crate::domain::PrFilter,
    pub search_query: String,
    pub loading: PrLoadingState,
    pub list_cursor: Option<String>,
    pub has_more_prs: bool,
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
    /// Content width (cols) for wrapping PR-detail text, read ONCE at the
    /// dispatch boundary (mirrors `detail_viewport_rows`). 0 means "no wrap".
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    pub detail_content_width: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    pub filter_ui: PrFilterUiState,
    pub search_input_focused: bool,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub draft_notice: Option<String>,
    pub mutation_pending: Option<PrMutationPending>,
    pub next_mutation_id: u64,
    pub list_reload_pending: Option<PrListReloadPending>,
    pub next_pr_list_request_id: u64,
    pub list_page_pending: Option<PrListPagePending>,
    pub detail_pending: Option<PrDetailPending>,
    pub next_pr_detail_request_id: u64,
    pub comments_page_pending: Option<PrCommentsPagePending>,
    pub next_comments_page_request_id: u64,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 88-98
/// Pending list-reload staleness guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListReloadPending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::PrFilter,
    pub request_id: u64,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-007
/// @pseudocode component-001 lines 88-98
/// Pending list-page staleness guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListPagePending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::PrFilter,
    pub cursor: Option<String>,
    pub request_id: u64,
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

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 88-98
/// Pending comments-page staleness guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrCommentsPagePending {
    pub scope_repo_id: RepositoryId,
    pub pr_number: u64,
    pub cursor: Option<String>,
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

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-006
/// PR loading flags (mirrors `IssueLoadingState`).
#[derive(Debug, Clone, Default)]
pub struct PrLoadingState {
    pub list: bool,
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
