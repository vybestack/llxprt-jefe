//! Runtime-only interaction state shared by the root reducer and UI.
//!
//! These types describe overlays, transient work, chooser state, and action
//! screen focus. They carry no declaration publication authority.

use super::types::PaneFocus;
use crate::domain::{AgentLaunchRequest, RepositoryId};

///
/// Tracks whether the temporary shell window is open and which agent it
/// belongs to. The overlay is runtime-only: it is not persisted, and closing
/// it restores the normal dashboard.
///
/// Issue #361 PR A: `inventory` mirrors every agent that currently owns a
/// live `jefe-shell` window (visible or hidden). The visible overlay is
/// still tracked by `agent_id`; the inventory lets F10 resume a hidden
/// shell and lets the background observer reconcile hidden exits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellOverlayState {
    /// The agent whose session hosts the currently *visible* temporary shell
    /// window. `None` means no shell overlay is visible (the dashboard owns
    /// the layout). A hidden shell still exists in `inventory`.
    pub agent_id: Option<crate::domain::AgentId>,
    /// Monotonic identity for an open/resume operation, used to reject stale observers.
    pub generation: u64,
    /// Dashboard pane focus restored when the visible shell hides/closes.
    pub previous_pane_focus: Option<PaneFocus>,
    /// Runtime-only inventory of every agent owning a live `jefe-shell`
    /// window, visible or hidden (issue #361). Updated only after runtime
    /// success/disappearance.
    pub inventory: super::ShellInventory,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 01-05
/// Focus domain within Issues Mode — separate from PaneFocus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IssueFocus {
    RepoList,
    #[default]
    IssueList,
    IssueDetail,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-003
/// Subfocus within issue detail view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailSubfocus {
    #[default]
    Body,
    Comment(usize),
    NewComment,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Inline mutable control state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InlineState {
    #[default]
    None,
    Composer {
        target: ComposerTarget,
        text: String,
        cursor: usize,
    },
    Editor {
        target: EditorTarget,
        text: String,
        cursor: usize,
    },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Target for inline composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerTarget {
    NewIssue,
    NewComment,
    Reply {
        comment_index: usize,
        author: String,
    },
    /// Reply to a PR review thread (issue #119). `thread_index` is the flat
    /// index across all reviews' threads, matching `PrDetailSubfocus::ReviewThread`.
    /// `thread_id` is the stable node id captured at open time so the dispatch
    /// layer can target the correct thread even after a reorder (issue #238).
    ReplyToReviewThread {
        thread_index: usize,
        thread_id: String,
        author: String,
    },
    /// Create a single-line review comment on an exact diff side.
    NewReviewThread {
        target: crate::domain::PrReviewCommentTarget,
    },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-010
/// Target for inline editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTarget {
    IssueBody,
    Comment { comment_index: usize },
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-011
/// State for send-to-agent chooser overlay.
///
/// The `agents` vector carries typed [`AgentChooserEntry`] snapshots built at
/// the `app_input` boundary (where git probing is permitted). Reducers only
/// validate non-emptiness and open/close/navigate — they never execute git.
///
/// When `transient_available` is true, an additional "Transient Agent" entry
/// appears after all regular agents at index `agents.len()` (issue #213).
/// Navigation bounds become `agents.len() + transient_available as usize`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentChooserState {
    pub selected_index: usize,
    pub agents: Vec<crate::domain::AgentChooserEntry>,
    /// Whether the transient-agent slot is available (issue #213).
    pub transient_available: bool,
}

/// What to send to a transient agent (issue #213).
///
/// Mirrors the issue/PR send paths: the payload is the same `SendPayload` /
/// `PrSendPayload` that the regular send orchestration consumes.
#[derive(Debug, Clone)]
pub enum TransientPayload {
    Issue {
        payload: crate::github::SendPayload,
    },
    PullRequest {
        payload: crate::github::PrSendPayload,
    },
}

/// A queued transient agent send waiting for a slot (issue #213).
///
/// When `transient_max_concurrent` is reached, the send context is captured
/// here and replayed when a running transient agent completes.
#[derive(Debug, Clone)]
pub struct QueuedTransientSend {
    pub repository_id: RepositoryId,
    pub work_dir: std::path::PathBuf,
    pub launch_signature: AgentLaunchRequest,
    pub payload: TransientPayload,
}

/// Queue of pending transient agent sends (issue #213).
///
/// Runtime-only — never persisted.
#[derive(Debug, Clone, Default)]
pub struct TransientAgentQueue {
    pub pending: Vec<QueuedTransientSend>,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-005
/// Saved agent-mode focus for restoration on exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorAgentFocus {
    pub pane_focus: PaneFocus,
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
}

/// Focus areas within GitHub Actions mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionsFocus {
    RepoList,
    #[default]
    RunList,
    Detail,
}

/// Filter field identifier for Actions UpdateDraftFilter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionsFilterField {
    Workflow,
    Status,
    Pr,
}

/// Identity for the Actions runs list — a result is stale unless both the
/// scope repo and the committed filter match exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsListIdentity {
    /// Repository scope the list was loaded for.
    pub scope_repo_id: RepositoryId,
    /// Committed filter snapshot when the load was started.
    pub filter: crate::domain::ActionsFilter,
}

/// Loading/pending state for Actions mode async operations.
///
/// List loading is now derived from `ActionsState::list` (the
/// `PaginatedList::is_loading()` / `has_pending_request()` accessors). Only
/// detail loading remains as an explicit flag here.
#[derive(Debug, Clone, Default)]
pub struct ActionsLoadingState {
    pub detail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsDispatchPending {
    pub scope_repo_id: crate::domain::RepositoryId,
    pub workflow_id: String,
    pub request_id: u64,
}

/// UI control state for Actions mode filter/search overlays.
#[derive(Debug, Clone, Default)]
pub struct ActionsUiState {
    pub filter_ui_open: bool,
    pub search_input_focused: bool,
    /// Active field index in the filter bar (0 = workflow, 1 = status, 2 = pr).
    /// Mirrors `issues_state.filter_ui.field_index` so the Actions filter bar
    /// renders field-active highlighting through the generic `FilterBar`.
    pub filter_field_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ActionsState {
    pub active: bool,
    /// Unified list state: runs, selection, pagination continuation, and
    /// pending load correlation. List loading is derived from this container.
    pub list:
        crate::state::pagination::PaginatedList<crate::domain::WorkflowRun, ActionsListIdentity>,
    pub run_detail: Option<crate::domain::WorkflowRunDetail>,
    pub workflows: Vec<crate::domain::Workflow>,
    pub committed_filter: crate::domain::ActionsFilter,
    pub draft_filter: crate::domain::ActionsFilter,
    pub search_query: String,
    /// Active sort configuration for the Actions runs list (issue #473).
    /// Lives on `ActionsState` because sort is a projection-time view
    /// transform — changing it must not re-run the fetch.
    pub sort_config: crate::domain::ActionsSortConfig,
    pub error: Option<String>,
    pub focus: ActionsFocus,
    pub detail_scroll_offset: usize,
    /// Last synchronized wrapped display-row viewport height.
    pub detail_viewport_rows: usize,
    /// Last synchronized content width used by the Actions wrap projection.
    pub detail_content_width: usize,
    /// Job ids that are expanded (showing their steps). Jobs not in this set
    /// are collapsed (JobRow only). Defaults to empty (all collapsed).
    pub expanded_jobs: std::collections::HashSet<u64>,
    /// Focused job index within the detail pane's job list (for keyboard
    /// navigation of expand/collapse). `None` when no detail is loaded.
    pub focused_job_index: Option<usize>,
    pub detail_pending: Option<ActionsDetailPending>,
    pub next_detail_request_id: u64,
    pub workflows_pending: Option<WorkflowsPending>,
    pub next_workflows_request_id: u64,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub dispatch_pending: Option<ActionsDispatchPending>,
    pub next_dispatch_request_id: u64,
    /// Decomposed loading/pending state (detail-only now).
    pub loading: ActionsLoadingState,
    /// Decomposed UI control state.
    pub ui: ActionsUiState,
}

impl ActionsState {
    #[must_use]
    pub fn dispatch_pending(&self) -> bool {
        self.dispatch_pending.is_some()
    }

    /// Read-only access to the loaded runs.
    #[must_use]
    pub fn runs(&self) -> &[crate::domain::WorkflowRun] {
        self.list.items()
    }

    /// The currently selected run index, if any.
    #[must_use]
    pub fn selected_run_index(&self) -> Option<usize> {
        self.list.selected_index()
    }

    /// The selected run when the stored index still names a loaded item.
    #[must_use]
    pub fn selected_run(&self) -> Option<&crate::domain::WorkflowRun> {
        self.selected_run_index()
            .and_then(|index| self.runs().get(index))
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
    pub fn has_more(&self) -> bool {
        self.list.has_more()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsDetailPending {
    pub scope_repo_id: RepositoryId,
    pub run_id: u64,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowsPending {
    pub scope_repo_id: RepositoryId,
    pub request_id: u64,
}

/// Which property of an issue the user is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuePropertyKind {
    Labels,
    Assignees,
    Milestone,
    Title,
    Type,
    State,
}

/// Which property of a PR the user is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrPropertyKind {
    Labels,
    Assignees,
    Milestone,
    Title,
    State,
}

/// A selectable option in the property editor list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyOption {
    pub label: String,
    pub selected: bool,
    /// Opaque node ID for issue types (None for other kinds). Display uses
    /// `label`; the mutation submits `id` (H2 fix).
    pub id: Option<String>,
}

/// Pending property mutation staleness guard (issue #175, H4 fix).
///
/// Mirrors `IssueMutationPending` / `PrMergeMutationPending`. Prevents
/// duplicate confirmations and ensures stale completions are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyMutationPending {
    pub scope_repo_id: crate::domain::RepositoryId,
    pub request_id: u64,
    pub number: u64,
}

/// Property editor overlay state for issues (mirrors `PrMergeChooserState`).
#[derive(Debug, Clone)]
pub struct IssuePropertyEditorState {
    pub kind: IssuePropertyKind,
    pub options: Vec<PropertyOption>,
    pub selected_index: usize,
    pub title_text: String,
    pub title_cursor: usize,
    pub error: Option<String>,
    /// Baseline labels/assignees currently applied (for diff computation, M8).
    pub baseline: Vec<String>,
    /// Whether the background options fetch failed (H5). When true, confirm is
    /// disabled to prevent destructive writes from missing data.
    pub loading_failed: bool,
    /// Whether options are still loading (M6). Set true on open, false on
    /// load-success/load-failure. Confirm is blocked while true.
    pub options_loading: bool,
    /// Request ID for the in-flight options load (M6 correlation).
    pub load_request_id: u64,
}

/// Property editor overlay state for PRs.
#[derive(Debug, Clone)]
pub struct PrPropertyEditorState {
    pub kind: PrPropertyKind,
    pub options: Vec<PropertyOption>,
    pub selected_index: usize,
    pub title_text: String,
    pub title_cursor: usize,
    pub error: Option<String>,
    /// Baseline labels/assignees currently applied (for diff computation, M8).
    pub baseline: Vec<String>,
    /// Whether the background options fetch failed (H5). When true, confirm is
    /// disabled to prevent destructive writes from missing data.
    pub loading_failed: bool,
    /// Whether options are still loading (M6). Set true on open, false on
    /// load-success/load-failure. Confirm is blocked while true.
    pub options_loading: bool,
    /// Request ID for the in-flight options load (M6 correlation).
    pub load_request_id: u64,
}
