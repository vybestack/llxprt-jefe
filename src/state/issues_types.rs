//! Issues Mode aggregate state types (extracted from types.rs).
//!
//! @plan PLAN-20260329-ISSUES-MODE.P03
//! @requirement REQ-ISS-001
//! @requirement REQ-ISS-003
//! @requirement REQ-ISS-005
//! @requirement REQ-ISS-010
//! @requirement REQ-ISS-011
//!
//! Mirrors `pr_types.rs`: these are the `IssuesState` aggregate + its helper
//! pending/loading/filter structs and the `impl IssuesState` viewport helpers.
//! The shared display-state enums (`IssueFocus`, `DetailSubfocus`,
//! `InlineState`, `ComposerTarget`, `EditorTarget`, `AgentChooserState`,
//! `PriorAgentFocus`) remain in `types.rs` and are imported via `super::`.

use crate::domain::{CloseReason, RepositoryId};

use super::{
    AgentChooserState, ComposerTarget, DetailSubfocus, InlineState, IssueFocus, PriorAgentFocus,
};

/// Aggregate state for Issues Mode.
#[derive(Debug, Clone, Default)]
pub struct IssuesState {
    pub active: bool,
    /// Unified list state: issues, selection, pagination continuation, and
    /// pending load correlation. List loading is derived from this container.
    pub list: crate::state::pagination::PaginatedList<crate::domain::Issue, IssueListIdentity>,
    pub issue_detail: Option<crate::domain::IssueDetail>,
    pub committed_filter: crate::domain::IssueFilter,
    pub draft_filter: crate::domain::IssueFilter,
    pub search_query: String,
    pub loading: IssueLoadingState,
    pub error: Option<String>,
    pub issue_focus: IssueFocus,
    pub detail_subfocus: DetailSubfocus,
    /// Scroll offset (in lines) for the detail pane viewport.
    pub detail_scroll_offset: usize,
    /// Last rendered detail viewport height in rows.
    pub detail_viewport_rows: usize,
    pub inline_state: InlineState,
    pub agent_chooser: Option<AgentChooserState>,
    pub filter_ui: IssueFilterUiState,
    pub search_input_focused: bool,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub draft_notice: Option<String>,
    pub mutation_pending: Option<IssueMutationPending>,
    pub next_mutation_id: u64,
    /// Delete confirm overlay state (two-step confirm like merge chooser).
    pub delete_confirm: Option<IssueDeleteConfirmState>,
    /// Close-reason chooser overlay state (issue #188).
    pub close_reason_chooser: Option<IssueCloseReasonChooserState>,
    /// Pending close mutation (single lifecycle pipeline; #175 coordination).
    pub close_mutation_pending: Option<IssueLifecycleMutationPending>,
    /// Pending delete mutation.
    pub delete_mutation_pending: Option<IssueLifecycleMutationPending>,
    pub detail_pending: Option<IssueDetailPending>,
    pub next_issue_detail_request_id: u64,
    pub comments_page_pending: Option<IssueCommentsPagePending>,
    pub next_comments_page_request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueListIdentity {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::IssueFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDetailPending {
    pub scope_repo_id: RepositoryId,
    pub issue_number: u64,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCommentsPagePending {
    pub scope_repo_id: RepositoryId,
    pub issue_number: u64,
    pub cursor: Option<String>,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueMutationPending {
    pub scope_repo_id: RepositoryId,
    /// Monotonic mutation id allocated from `next_mutation_id` (NOT the issue
    /// number). Used to match a success/failure result back to the in-flight
    /// mutation. Distinct from `IssueLifecycleMutationPending.mutation_id`
    /// (the close/delete pipeline) and `IssuesState.next_mutation_id` (the
    /// allocator).
    pub id: u64,
    pub target: InlineState,
}

/// Delete confirm overlay state (issue #182).
///
/// Two-step confirm like the PR merge chooser: the overlay opens with
/// `awaiting_confirmation == false`; the first `IssueDeleteConfirm` arms it,
/// and a second `IssueDeleteConfirm` dispatches the mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDeleteConfirmState {
    pub issue_number: u64,
    pub awaiting_confirmation: bool,
}

/// Close-reason chooser overlay state (issue #188). Mirrors the merge chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCloseReasonChooserState {
    pub issue_number: u64,
    pub selected_index: usize,
    /// When the chosen reason is Duplicate, the user types a number here.
    pub duplicate_search: Option<IssueDuplicateSearchState>,
    /// Two-step confirm like delete-confirm (avoids accidental close).
    pub awaiting_confirmation: bool,
}

/// Duplicate-by-number search sub-state (issue #188).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IssueDuplicateSearchState {
    pub query: String,
    /// Issues seeded from the repo's loaded issue list (number + title).
    pub candidates: Vec<(u64, String)>,
    pub selected_index: usize,
}

/// Pending close or delete mutation (issue #182 lifecycle pipeline).
///
/// `node_id` is `Some` for a delete (captured at confirm time from the
/// focused issue's node id) and `None` for a close (which closes by number).
/// Capturing the node id here means the dispatch layer reads it once from the
/// pending record instead of re-resolving it from mutable state, eliminating a
/// time-of-check/time-of-use seam and the duplicated resolution logic.
///
/// `close_reason` and `duplicate_of` carry the close-reason context (issue
/// #188). For the legacy plain-close path and deletes, both are `None`. For a
/// close-with-reason, `close_reason` is `Some(reason)`. For a Duplicate close,
/// `duplicate_of` is `Some(n)` (the canonical issue number).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueLifecycleMutationPending {
    pub scope_repo_id: RepositoryId,
    /// Monotonic mutation id allocated from `IssuesState.next_mutation_id`
    /// (the SAME shared allocator used by inline-composer mutations via
    /// `IssueMutationPending.id`), so close/delete ids can never collide
    /// with in-flight inline mutation ids.
    pub mutation_id: u64,
    pub issue_number: u64,
    pub node_id: Option<String>,
    pub close_reason: Option<CloseReason>,
    pub duplicate_of: Option<u64>,
}

/// Loading/pending state for Issues mode async operations.
///
/// List loading is derived from `IssuesState::list` (the
/// `PaginatedList::is_loading()` / `has_pending_request()` accessors). Only
/// detail and comments loading remain as explicit flags here.
#[derive(Debug, Clone, Default)]
pub struct IssueLoadingState {
    pub detail: bool,
    pub comments: bool,
}

pub const ISSUE_FILTER_FIELD_COUNT: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct IssueFilterUiState {
    pub controls_open: bool,
    /// Index of the currently focused filter field (0=state, 1=author, 2=assignee, 3=labels, 4=type, 5=milestone, 6=module, 7=query_text).
    pub field_index: usize,
    /// Raw labels text while editing (preserves trailing commas). Parsed into Vec on apply.
    pub draft_labels_text: String,
}

impl IssuesState {
    /// Read-only access to the loaded issues.
    #[must_use]
    pub fn issues(&self) -> &[crate::domain::Issue] {
        self.list.items()
    }

    /// The currently selected issue index, if any.
    #[must_use]
    pub fn selected_issue_index(&self) -> Option<usize> {
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
    pub fn has_more_issues(&self) -> bool {
        self.list.has_more()
    }

    /// Count the number of rendered content lines for the current detail view.
    #[must_use]
    pub fn detail_content_line_count(&self) -> usize {
        let Some(detail) = &self.issue_detail else {
            return 0;
        };

        crate::issue_detail_content::detail_content_line_count(
            detail,
            &self.inline_state,
            self.loading.comments,
        )
    }

    /// Maximum scroll offset so the last line of content sits at the bottom of the viewport.
    /// Returns 0 when content fits entirely within the viewport (no scrolling needed).
    #[must_use]
    pub fn max_detail_scroll_offset(&self) -> usize {
        let viewport_rows = if self.detail_viewport_rows == 0 {
            crate::layout::detail_viewport_rows(40)
        } else {
            self.detail_viewport_rows
        };
        self.max_detail_scroll_offset_for_viewport(viewport_rows)
    }

    /// Maximum detail scroll offset for a caller-provided viewport row count.
    #[must_use]
    pub fn max_detail_scroll_offset_for_viewport(&self, viewport_rows: usize) -> usize {
        if self.issue_detail.is_none() {
            return 0;
        }
        let composer_active = matches!(
            self.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment | ComposerTarget::Reply { .. },
                ..
            }
        );
        self.detail_content_line_count().saturating_sub(
            crate::layout::issue_detail_document_viewport_rows(viewport_rows, composer_active),
        )
    }

    /// Maximum detail scroll offset for the Issues-mode layout bands currently
    /// visible in the UI.
    #[must_use]
    pub fn max_detail_scroll_offset_for_layout(
        &self,
        term_rows: usize,
        error_visible: bool,
        filter_controls_open: bool,
    ) -> usize {
        self.max_detail_scroll_offset_for_viewport(crate::layout::issues_detail_viewport_rows(
            term_rows,
            error_visible,
            filter_controls_open,
        ))
    }
}
