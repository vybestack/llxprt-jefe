//! State types: structs, enums, and field definitions.

use std::time::Instant;

use crate::domain::{AgentId, LaunchSignature, RepositoryId};
use crate::runtime::PreflightIssue;

// @plan PLAN-20260624-PR-MODE.P03
#[path = "pr_types.rs"]
mod pr_types;
pub use pr_types::*;

// Form-field types extracted to keep this file under the length limit.
#[path = "form_types.rs"]
mod form_types;
pub use form_types::*;

/// Modal/form state variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ModalState {
    #[default]
    None,
    Help,
    Search {
        query: String,
    },
    /// Theme picker overlay.
    ///
    /// Lists available theme slugs/names for navigation + selection.
    /// `available_themes` is the snapshot of slugs+names captured when the
    /// picker was opened; the actual theme application happens via
    /// `AppEvent::SetTheme`, which the binary's dispatch layer applies to the
    /// `ThemeManager` and persists to `settings.toml`.
    ThemePicker {
        available_themes: Vec<(String, String)>,
        selected_index: usize,
        /// Slug of the currently-applied theme (for the active marker).
        active_slug: String,
        /// In-dialog "Apply jefe theme to agent" toggle (issue #179).
        /// Initialized from `AppState.override_agent_theme`; persisted on Enter.
        override_theme: bool,
    },
    NewRepository {
        fields: RepositoryFormFields,
        focus: RepositoryFormFocus,
        cursor: RepositoryFormCursor,
    },
    EditRepository {
        id: RepositoryId,
        fields: RepositoryFormFields,
        focus: RepositoryFormFocus,
        cursor: RepositoryFormCursor,
    },
    ConfirmDeleteRepository {
        id: RepositoryId,
    },
    NewAgent {
        repository_id: RepositoryId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
        cursor: AgentFormCursor,
        /// Track if work_dir was manually edited (stop auto-deriving from name).
        work_dir_manual: bool,
    },
    EditAgent {
        id: AgentId,
        fields: AgentFormFields,
        focus: AgentFormFocus,
        cursor: AgentFormCursor,
    },
    ConfirmDeleteAgent {
        id: AgentId,
        delete_work_dir: bool,
    },
    ConfirmKillAgent {
        id: AgentId,
    },
    /// Preflight check failed — prompt the user for remediation before launch.
    ///
    /// TODO(issue #24): Expand this to support a queue of issues if preflight
    /// transitions from single-issue checks to batched diagnostics.
    PreflightPrompt {
        /// The agent being launched (so we can resume after remediation).
        agent_id: AgentId,
        /// The launch signature (so we can resume the spawn).
        signature: LaunchSignature,
        /// The issue that was detected.
        issue: PreflightIssue,
        /// Placeholder for future multi-issue handling.
        remaining_issues: Vec<PreflightIssue>,
    },
    /// Issue send: the working copy has uncommitted changes (excluding
    /// jefe/llxprt-owned paths). Prompt the user to discard them before
    /// the issue-driven launch proceeds. The default is no/halt; the
    /// user must explicitly opt in (Enter) before destructive cleanup.
    /// Escape (or `n`) aborts and leaves the working copy untouched.
    ConfirmIssueDirtyCopy {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: LaunchSignature,
        payload: crate::github::SendPayload,
    },
    WorkflowDispatch {
        workflow: crate::domain::Workflow,
        fields: WorkflowDispatchFormFields,
        focus: WorkflowDispatchFormFocus,
        cursor: WorkflowDispatchFormCursor,
    },
}

/// Screen mode variants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Dashboard,
    Split,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-001
    DashboardIssues,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    /// @pseudocode component-001 lines 66-76
    DashboardPullRequests,
    DashboardActions,
}

/// Pane focus within a view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,
    Agents,
    Terminal,
}

/// In-progress dashboard reorder ("grab") target — tracks the visible-index
/// position of the grabbed item so arrow-move stays within the filtered/visible set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardGrabPane {
    /// Grabbing a repository at the given visible-index position.
    Repository { visible_index: usize },
    /// Grabbing an agent at the given local visible-index position within its repository.
    ///
    /// The `repository_id` is captured at grab time so the grab stays bound to
    /// the repository that was selected when Space was pressed — even if the
    /// selected repository changes (e.g. via a shortcut jump) while the grab
    /// is active.
    Agent {
        repository_id: RepositoryId,
        local_index: usize,
    },
}

/// Bookkeeping for the rapid `qqq` quit sequence.
///
/// Held in [`AppState`] so the count survives across key events. It is reset
/// on the inter-press timeout, on any non-`q` key, and whenever a quit fires.
/// The decision logic lives in `crate::input::observe_quit_sequence`; this type
/// only stores the accumulated state. Runtime-only — never persisted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QuitSequenceState {
    /// Consecutive rapid `q` presses accumulated toward the quit threshold.
    pub presses: u8,
    /// Instant of the most recent `q`, used to enforce the inter-press window.
    pub last_press: Option<Instant>,
}

/// Application state - single source of truth.
#[derive(Debug, Default, Clone)]
pub struct AppState {
    // Data
    pub repositories: Vec<crate::domain::Repository>,
    pub agents: Vec<crate::domain::Agent>,
    /// Runtime availability snapshot detected once during startup.
    pub installed_agent_kinds: Vec<crate::domain::AgentKind>,

    // Selection
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,

    // View state
    pub screen_mode: ScreenMode,
    pub pane_focus: PaneFocus,
    pub terminal_focused: bool,
    pub hide_idle_repositories: bool,

    /// Agent IDs that were just killed and should remain visible in active-only
    /// mode until the user navigates away. Runtime-only — not persisted.
    pub sticky_dead_agent_ids: std::collections::HashSet<crate::domain::AgentId>,

    // Modal/form state
    pub modal: ModalState,

    // Split mode state
    pub split_filter: Option<RepositoryId>,
    pub split_grab_index: Option<usize>,

    /// Active dashboard reorder grab (Space to grab, arrows to move, Space/Enter to drop).
    /// Transient interaction state — not persisted (like split_grab_index).
    pub dashboard_grab: Option<DashboardGrabPane>,

    // Errors/warnings
    pub error_message: Option<String>,
    pub warning_message: Option<String>,

    // Issues mode state
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-001
    pub issues_state: IssuesState,

    // PR mode state (runtime-only — omitted from persisted DTO, same as issues_state)
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-001
    pub prs_state: PullRequestsState,

    /// Per-repository remembered user preferences (issue #163).
    ///
    /// Runtime copy of the persisted DTO — mirror of
    /// `persistence::State.user_preferences`. The reducer reads/writes this
    /// in memory; the app-shell persists it via `to_persisted_state`.
    pub user_preferences: crate::domain::UserPreferences,

    /// GitHub Actions mode state (runtime-only — omitted from persisted DTO).
    pub actions_state: ActionsState,

    /// Rapid `qqq` quit-sequence bookkeeping. Runtime-only — never persisted.
    pub quit_sequence: QuitSequenceState,

    /// Active mouse text-selection, if any. Runtime-only — never persisted.
    ///
    /// Set by the app-shell mouse router when the user drag-selects text in any
    /// pane (or in the terminal snapshot when unfocused). Cleared on Escape or
    /// when a new selection begins. Used by the renderers to paint an
    /// inverse-video highlight over the selected cells.
    pub selection: Option<crate::selection::TextSelection>,

    /// The terminal snapshot bound to the active selection (issue #197).
    ///
    /// Captured when a terminal selection gesture begins and reused for BOTH
    /// the highlight rendering and the copy-at-release so copied text always
    /// matches what the user highlighted — even when the live grid streams new
    /// output between the last drag frame and mouse-up. Cleared together with
    /// `selection`. Runtime-only — never persisted.
    pub selection_snapshot: Option<crate::runtime::TerminalSnapshot>,

    /// Gesture-ownership state for the terminal mouse router (issue #197).
    ///
    /// Persists the left-button gesture ownership decision (Jefe vs PTY) across
    /// events within a single down→drag→up cycle. Reset to idle on release.
    /// Runtime-only — never persisted.
    pub terminal_gesture_state: crate::selection::GestureState,

    /// Help modal scroll offset (lines scrolled from the top). Mirrored from
    /// the app-shell hook state so the selection content projection can map
    /// screen coordinates to the correct help content line (issue #178).
    /// Runtime-only — never persisted.
    pub help_scroll_offset: usize,

    /// Terminal scrollback offset for the embedded terminal pane (issue #198).
    ///
    /// `None` (default) means **follow-tail**: render the live snapshot at the
    /// bottom (current behavior). `Some(n)` means the viewport is scrolled back
    /// `n` lines from the bottom; follow-tail is paused and a follow indicator
    /// renders. Runtime-only — never persisted (like `selection`,
    /// `quit_sequence`).
    pub terminal_history_offset: Option<usize>,

    /// Cached number of terminal viewport rows (for scrollback offset math,
    /// issue #198). Mirrors `detail_viewport_rows` for detail panes. Updated by
    /// the render/layout layer so the deterministic reducer can compute clamp
    /// bounds without I/O. Runtime-only — never persisted.
    pub terminal_viewport_rows: usize,

    /// Cached total lines of scrollback content (history + live snapshot rows,
    /// issue #198). Updated by the render layer from the runtime history
    /// capture + live snapshot. Runtime-only — never persisted.
    pub terminal_total_lines: usize,

    /// Runtime mirror of `persistence::Settings.override_agent_theme` (issue
    /// #179). settings.toml is the source of truth; the render path reads this.
    pub override_agent_theme: bool,
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
    ReplyToReviewThread {
        thread_index: usize,
        author: String,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentChooserState {
    pub selected_index: usize,
    pub agents: Vec<(crate::domain::AgentId, String)>,
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
}

/// Loading/pending state for Actions mode async operations.
#[derive(Debug, Clone, Default)]
pub struct ActionsLoadingState {
    pub list: bool,
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
    /// Active field index in the filter bar (0 = workflow, 1 = status).
    /// Mirrors `issues_state.filter_ui.field_index` so the Actions filter bar
    /// renders field-active highlighting through the generic `FilterBar`.
    pub filter_field_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ActionsState {
    pub active: bool,
    pub runs: Vec<crate::domain::WorkflowRun>,
    pub selected_run_index: Option<usize>,
    pub run_detail: Option<crate::domain::WorkflowRunDetail>,
    pub workflows: Vec<crate::domain::Workflow>,
    pub committed_filter: crate::domain::ActionsFilter,
    pub draft_filter: crate::domain::ActionsFilter,
    pub search_query: String,
    pub error: Option<String>,
    pub page: u32,
    pub focus: ActionsFocus,
    pub detail_scroll_offset: usize,
    pub detail_viewport_rows: usize,
    /// Job ids that are expanded (showing their steps). Jobs not in this set
    /// are collapsed (JobRow only). Defaults to empty (all collapsed).
    pub expanded_jobs: std::collections::HashSet<u64>,
    /// Focused job index within the detail pane's job list (for keyboard
    /// navigation of expand/collapse). `None` when no detail is loaded.
    pub focused_job_index: Option<usize>,
    pub list_reload_pending: Option<ActionsListReloadPending>,
    pub next_list_request_id: u64,
    pub detail_pending: Option<ActionsDetailPending>,
    pub next_detail_request_id: u64,
    pub workflows_pending: Option<WorkflowsPending>,
    pub next_workflows_request_id: u64,
    pub prior_agent_focus: Option<PriorAgentFocus>,
    pub dispatch_pending: Option<ActionsDispatchPending>,
    pub next_dispatch_request_id: u64,
    /// Pagination marker received from the last list load. Currently only page
    /// 1 is loaded (no load-more path exists); this field is retained for
    /// future pagination support and is never read for any load decision.
    pub has_more: bool,
    /// Decomposed loading/pending state.
    pub loading: ActionsLoadingState,
    /// Decomposed UI control state.
    pub ui: ActionsUiState,
}

impl ActionsState {
    #[must_use]
    pub fn dispatch_pending(&self) -> bool {
        self.dispatch_pending.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionsListReloadPending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::ActionsFilter,
    pub page: u32,
    pub request_id: u64,
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

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 33-40
/// Aggregate state for Issues Mode.
#[derive(Debug, Clone, Default)]
pub struct IssuesState {
    pub active: bool,
    pub issues: Vec<crate::domain::Issue>,
    pub selected_issue_index: Option<usize>,
    pub issue_detail: Option<crate::domain::IssueDetail>,
    pub committed_filter: crate::domain::IssueFilter,
    pub draft_filter: crate::domain::IssueFilter,
    pub search_query: String,
    pub loading: IssueLoadingState,
    pub list_cursor: Option<String>,
    pub has_more_issues: bool,
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
    pub list_reload_pending: Option<IssueListReloadPending>,
    pub next_issue_list_request_id: u64,
    pub list_page_pending: Option<IssueListPagePending>,
    pub detail_pending: Option<IssueDetailPending>,
    pub next_issue_detail_request_id: u64,
    pub comments_page_pending: Option<IssueCommentsPagePending>,
    pub next_comments_page_request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueListReloadPending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::IssueFilter,
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueListPagePending {
    pub scope_repo_id: RepositoryId,
    pub filter: crate::domain::IssueFilter,
    pub cursor: Option<String>,
    pub request_id: u64,
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
    pub id: u64,
    pub target: InlineState,
}

#[derive(Debug, Clone, Default)]
pub struct IssueLoadingState {
    pub list: bool,
    pub detail: bool,
    pub comments: bool,
}

pub const ISSUE_FILTER_FIELD_COUNT: usize = 8;

/// Number of PR filter fields for FilterNavigate wrap (issue #163).
pub const PR_FILTER_FIELD_COUNT: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct IssueFilterUiState {
    pub controls_open: bool,
    /// Index of the currently focused filter field (0=state, 1=author, 2=assignee, 3=labels, 4=type, 5=milestone, 6=module, 7=query_text).
    pub field_index: usize,
    /// Raw labels text while editing (preserves trailing commas). Parsed into Vec on apply.
    pub draft_labels_text: String,
}

impl IssuesState {
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
