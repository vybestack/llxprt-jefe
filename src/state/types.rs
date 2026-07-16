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

// Issues-mode aggregate state extracted to keep this file under the length limit.
#[path = "issues_types.rs"]
mod issues_types;
pub use issues_types::*;

// `ISSUE_FILTER_FIELD_COUNT` lives in `issues_types.rs`; the PR sibling stays
// here so each mode filter references its own count.
/// Number of PR filter fields for FilterNavigate wrap (issue #163).
pub const PR_FILTER_FIELD_COUNT: usize = 8;

/// Captured issue self-assignment follow-up for an issue-driven launch
/// (issue #186).
///
/// Carried through the preflight modal so the non-blocking
/// assignment (or its warning) fires after a successful post-preflight
/// launch.
///
/// - [`IssueSelfAssignmentFollowUp::Resolved`]: a valid `owner/repo` was
///   resolved from the agent's repository; the background task will resolve
///   the viewer and POST the assignment.
/// - [`IssueSelfAssignmentFollowUp::Unavailable`]: the repository has no valid
///   `github_repo`, so assignment cannot run; a non-blocking warning must be
///   surfaced instead of silently skipping (consistent with the direct path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueSelfAssignmentFollowUp {
    Resolved {
        /// Validated `owner/repo` shortform (never the slug).
        owner_repo: String,
        issue_number: u64,
    },
    Unavailable {
        issue_number: u64,
        reason: String,
    },
}

/// Which button is focused in a confirm dialog (issue #228).
///
/// Defaults to [`ConfirmFocus::Cancel`] so destructive confirms are
/// defense-in-depth: Enter on a freshly-opened dialog does nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfirmFocus {
    #[default]
    Cancel,
    Confirm,
}

/// Phase of the in-app device-code auth dialog state machine (issue #244).
///
/// The dialog drives `gh auth login --web` non-interactively; these phases
/// track where the flow is so the UI is render-only and the reducer stays
/// deterministic.
///
/// `Debug` is implemented manually to redact the one-time device code: it is
/// a short-lived bearer credential while valid, so it must never leak through
/// `AppState` debug logs, crash reports, or test snapshots.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthDialogPhase {
    /// Dialog not shown (modal closed).
    Idle,
    /// `gh auth login` subprocess spawned; waiting for the one-time code to
    /// be parsed from its stderr.
    AwaitingCode,
    /// Code + URL have been parsed and shown to the user; the subprocess is
    /// polling until the user authorizes in a browser.
    Confirming { code: String, url: String },
    /// A transient failure occurred (network, code expiry); a retry is offered.
    Failed { error: String, can_retry: bool },
    /// The user cancelled (Esc); the modal is being dismissed.
    Cancelled,
}

impl std::fmt::Debug for AuthDialogPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("AuthDialogPhase::Idle"),
            Self::AwaitingCode => f.write_str("AuthDialogPhase::AwaitingCode"),
            Self::Confirming { url, .. } => f
                .debug_struct("AuthDialogPhase::Confirming")
                .field("code", &"<redacted>")
                .field("url", url)
                .finish(),
            Self::Failed { error, can_retry } => {
                // Defense-in-depth: the dispatch layer already scrubs the code
                // shape before storing, but redact again here so a future caller
                // cannot leak a one-time code via a Debug print (issue #244).
                let redacted = crate::github::redact_device_codes(error);
                f.debug_struct("AuthDialogPhase::Failed")
                    .field("error", &redacted)
                    .field("can_retry", can_retry)
                    .finish()
            }
            Self::Cancelled => f.write_str("AuthDialogPhase::Cancelled"),
        }
    }
}

/// State carried by [`ModalState::Auth`].
///
/// Runtime-only — never persisted (auth is an interactive, ephemeral flow).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthDialogState {
    pub phase: AuthDialogPhase,
}

impl Default for AuthDialogState {
    fn default() -> Self {
        Self {
            phase: AuthDialogPhase::Idle,
        }
    }
}

impl AuthDialogState {
    /// Construct a fresh dialog in the [`AuthDialogPhase::AwaitingCode`]
    /// phase — the entry point when the auth flow starts.
    #[must_use]
    pub fn awaiting_code() -> Self {
        Self {
            phase: AuthDialogPhase::AwaitingCode,
        }
    }
}

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
        confirm_focus: ConfirmFocus,
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
        confirm_focus: ConfirmFocus,
    },
    ConfirmKillAgent {
        id: AgentId,
        confirm_focus: ConfirmFocus,
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
        /// Captured issue self-assignment follow-up for issue-driven launches
        /// (issue #186). `None` for non-issue launches (e.g. relaunch). When
        /// present, the assignment (or its warning) fires after a successful
        /// post-preflight launch.
        issue_self_assignment: Option<IssueSelfAssignmentFollowUp>,
        confirm_focus: ConfirmFocus,
    },
    /// Issue send: the working copy is dirty (uncommitted changes, excluding
    /// jefe/llxprt-owned paths) OR not on the repository's default branch
    /// (issue #338). Prompt the user to switch to the default branch, discard
    /// non-owned changes, and pull before the issue-driven launch proceeds.
    /// The default is no/halt; the user must explicitly opt in (Enter) before
    /// destructive cleanup. Escape (or `n`) aborts and leaves the working
    /// copy untouched.
    ConfirmIssueDirtyCopy {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: LaunchSignature,
        payload: crate::github::SendPayload,
        confirm_focus: ConfirmFocus,
    },
    /// Issue send: the working copy is a git repo whose `origin` does not
    /// match the configured repository. Prompt the user to replace it with a
    /// fresh clone before the issue-driven launch proceeds. The default is
    /// no/halt; the user must explicitly opt in (Enter) before the
    /// destructive remove+reclone. Escape (or `n`) aborts and leaves the
    /// working copy untouched.
    ConfirmIssueOriginMismatch {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: LaunchSignature,
        payload: crate::github::SendPayload,
        actual: String,
        expected: String,
        confirm_focus: ConfirmFocus,
    },
    WorkflowDispatch {
        workflow: crate::domain::Workflow,
        fields: WorkflowDispatchFormFields,
        focus: WorkflowDispatchFormFocus,
        cursor: WorkflowDispatchFormCursor,
    },
    /// In-app device-code auth remediation dialog (issue #244). Render-only
    /// data: the runtime layer owns the `gh auth login --web` subprocess.
    Auth {
        state: AuthDialogState,
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
    DashboardErrors,
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

    /// Errors-mode state (runtime-only — omitted from persisted DTO).
    /// Captures the last N errors for the dedicated errors panel (issue #292).
    pub errors_state: super::ErrorsState,

    /// Rapid `qqq` quit-sequence bookkeeping. Runtime-only — never persisted.
    pub quit_sequence: QuitSequenceState,

    /// Active mouse text-selection, if any. Runtime-only — never persisted.
    ///
    /// Set by the app-shell mouse router when the user drag-selects text in any
    /// pane (or in the terminal snapshot when unfocused). Cleared on Escape or
    /// when a new selection begins. Used by the renderers to paint an
    /// inverse-video highlight over the selected cells.
    pub selection: Option<crate::selection::TextSelection>,

    /// Git display data bound to an active dashboard selection gesture.
    pub selection_dashboard_git_info: Option<crate::dashboard_git_info::DashboardGitInfoSnapshot>,

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

    /// Pending transient-agent sends queued because max_concurrent is reached
    /// (issue #213). Runtime-only — never persisted.
    pub transient_queue: TransientAgentQueue,

    /// Embedded agent-shell overlay state (issue #222). When active, the
    /// dashboard replaces the agent list + preview with the shell terminal
    /// pane while preserving the repository sidebar and outer bars.
    /// Runtime-only — never persisted.
    pub shell_overlay: ShellOverlayState,
}

/// Embedded agent-shell overlay state (issue #222).
///
/// Tracks whether the temporary shell window is open and which agent it
/// belongs to. The overlay is runtime-only: it is not persisted, and closing
/// it restores the normal dashboard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellOverlayState {
    /// The agent whose session hosts the temporary shell window. `None` means
    /// the overlay is inactive.
    pub agent_id: Option<crate::domain::AgentId>,
    /// Monotonic identity for an open operation, used to reject stale observers.
    pub generation: u64,
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
    pub launch_signature: LaunchSignature,
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
