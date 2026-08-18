//! State types: structs, enums, and field definitions.

use std::time::Instant;

use super::workbench_filter::WorkbenchUiState;
use crate::domain::{AgentId, AgentLaunchRequest, RepositoryId};
use crate::runtime::PreflightIssue;

// Which screen is active is the workbench's vocabulary: identity is the stable
// namespaced string that descriptors, persistence, and goldens agree on. State
// re-exports it so consumers keep reaching for it through `crate::state`.
pub use crate::workbench::{ScreenId, ScreenIdentity};

// @plan PLAN-20260624-PR-MODE.P03
#[path = "pr_types.rs"]
mod pr_types;
pub use pr_types::*;

#[path = "form_types.rs"]
mod form_types;
pub use form_types::*;

// New Issue dialog form-field types extracted for issue #407.
#[path = "new_issue_form_types.rs"]
mod new_issue_types;
pub use new_issue_types::{IssueType, NewIssueFormFocus, NewIssueFormState, NewIssueTemplate};

// Issues-mode aggregate state extracted to keep this file under the length limit.
#[path = "issues_types.rs"]
mod issues_types;
pub use issues_types::*;

// `ISSUE_FILTER_FIELD_COUNT` lives in `issues_types.rs`; the PR sibling stays
// here so each mode filter references its own count.
/// Number of PR filter fields for FilterNavigate wrap (issue #163).
///
/// Includes the two sort fields (sort-by, sort-order) appended for issue #473:
/// 0 state, 1 draft, 2 review, 3 checks, 4 author, 5 assignee, 6 reviewer,
/// 7 labels, 8 sort_by, 9 sort_order.
pub const PR_FILTER_FIELD_COUNT: usize = 10;

/// Index of the sort-by cycle field in the Actions filter bar (0-based,
/// after the 3 filter fields: workflow, status, pr — issue #473).
pub const ACTIONS_SORT_BY_FIELD_INDEX: usize = 3;
/// Index of the sort-order cycle field in the Actions filter bar.
pub const ACTIONS_SORT_ORDER_FIELD_INDEX: usize = 4;

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
    /// Definition-driven New Agent form opened from the Agent Types surface.
    GeneratedAgent {
        /// Selected definition/type ID captured at open time so the canonical
        /// submit path retains the sole authority even after the form result
        /// is consumed.
        type_id: Box<crate::domain::agent_definition::AgentTypeId>,
        form: Box<super::generated_agent_form::GeneratedAgentForm>,
        return_focus: PaneFocus,
        return_agent_type_index: usize,
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
    ConfirmServerLostRecovery {
        agent_ids: Vec<AgentId>,
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
        signature: AgentLaunchRequest,
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
        signature: AgentLaunchRequest,
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
        signature: AgentLaunchRequest,
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

/// Dashboard "search lite" input + query state (issue #405).
///
/// Grouped so [`AppState`] stays within the `max-struct-bools` budget. The
/// query live-filters the repository sidebar and agent pane by `name`
/// (case-insensitive substring), AND-composed with `hide_idle_repositories`.
/// Runtime-only — never persisted (a stale filter on startup would contradict
/// "make it obvious you're filtered").
#[derive(Debug, Default, Clone)]
pub struct DashboardSearchState {
    /// Current query text.
    pub query: String,
    /// Pre-lowered query for fast case-insensitive matching (issue #405 OCR).
    pub lowered_query: String,
    /// Whether the search input has keyboard focus.
    pub input_focused: bool,
}

/// Active-only visibility exceptions created by recent UI events, grouped so
/// [`AppState`] stays within its complexity budget.
///
/// Just-killed agents (issue #116) and just-created repositories that have no
/// agents yet (issue #404) stay visible in active-only mode until the user
/// navigates away, so a kill or a create is never invisible feedback.
/// Runtime-only — never persisted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StickyVisibilityState {
    /// Agent IDs that were just killed.
    pub dead_agents: std::collections::HashSet<crate::domain::AgentId>,
    /// Repository IDs that were just created and have no agents yet.
    pub empty_repositories: std::collections::HashSet<RepositoryId>,
}

/// Application state - single source of truth.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The one immutable declaration aggregate committed before this state was
    /// constructed. Private so reducers cannot replace session declarations.
    published_workbench: std::sync::Arc<crate::published_workbench::PublishedWorkbench>,

    // Data
    pub repositories: Vec<crate::domain::Repository>,
    pub agents: Vec<crate::domain::Agent>,
    /// Enabled, compatible agent types observed by the startup probe boundary.
    pub available_agent_type_ids: Vec<crate::domain::agent_definition::AgentTypeId>,
    /// Definition-driven runtime availability observed once during startup.
    pub agent_type_availability: Vec<crate::agent_status_view::AgentAvailabilityObservation>,
    /// Monotonic generation allocated by the state-owned availability boundary.
    pub agent_probe_generation: u64,

    /// Payload-preserving JSP observations keyed by Jefe agent identity.
    /// Runtime-only: never projected into persistence.
    pub observations: std::collections::HashMap<
        crate::domain::AgentId,
        crate::domain::observation::AgentObservation,
    >,
    /// Highest JSP lifecycle generation accepted or explicitly cleared for
    /// each agent. Retaining the tombstone rejects delayed queued messages.
    pub observation_generations: std::collections::HashMap<crate::domain::AgentId, u64>,

    // Selection
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
    /// Runtime-only selected row in the Agent Types status surface.
    pub selected_agent_type_index: usize,
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,

    // View state
    /// Where the session is, and where it has been.
    ///
    /// The sole runtime authority for screen identity (issue #386). Nothing
    /// assigns a screen directly any more: every screen change goes through
    /// `crate::state::navigation::reduce_navigation`, so the stack, the
    /// generations that decide which answers are still wanted, and the dirty
    /// guard cannot disagree with what is on screen. Runtime-only — the
    /// durable document remembers a screen, never a stack.
    pub nav: super::navigation::NavState,
    pub pane_focus: PaneFocus,
    pub terminal_focused: bool,
    pub hide_idle_repositories: bool,
    /// Multi-agent workbench view state (issue #626); runtime-only.
    pub workbench: WorkbenchUiState,

    /// Dashboard "search lite" state for repositories and agents (issue #405).
    ///
    /// Filters the sidebar (by repo `name`) AND the agent pane (by agent
    /// `name`), case-insensitively, AND-composed with
    /// `hide_idle_repositories`. Runtime-only — never persisted (a stale
    /// filter on startup would contradict "make it obvious you're filtered").
    pub dashboard_search: DashboardSearchState,

    /// Active-only visibility exceptions (issue #116 for just-killed agents,
    /// issue #404 for just-created repositories). Runtime-only — never
    /// persisted.
    pub sticky_visibility: StickyVisibilityState,

    // Modal/form state
    pub modal: ModalState,

    // Split mode state
    pub split_filter: Option<RepositoryId>,
    pub split_grab_index: Option<usize>,

    /// Active dashboard reorder grab (Space to grab, arrows to move, Space/Enter to drop).
    /// Transient interaction state — not persisted (like split_grab_index).
    pub dashboard_grab: Option<DashboardGrabPane>,
    /// Runtime-only action availability generation layered over the immutable
    /// action declarations committed in `published_workbench`.
    pub(crate) action_availability: Option<crate::domain::action_registry::AvailabilityGeneration>,
    /// Runtime-only health-derived provider availability overrides.
    pub provider_action_health:
        std::collections::BTreeMap<crate::domain::action_registry::ActionId, String>,
    /// Provider action whose unavailable result currently owns the provider surface.
    pub provider_surface_action: Option<crate::domain::action_registry::ActionId>,
    /// Most recent provider notice accepted by the post-commit host adapter.
    pub provider_notice: Option<crate::domain::effects::ProviderNotice>,
    /// Runtime-only provider panel lifecycle and complete accepted models.
    pub provider_panels: super::provider_panels::ProviderPanelState,
    /// One root-owned immutable geometry snapshot for the active screen.
    /// Resolved once per size or state change and read by every geometry
    /// consumer — renderer, mouse routing, selection, wrapping, scrolling, and
    /// PTY resize — so no two can disagree about where a panel is. Runtime-only
    /// and never persisted: geometry is derived, not remembered.
    pub resolved_layout: Option<crate::workbench::ResolvedLayout>,

    // Errors/warnings
    pub error_message: Option<String>,
    pub warning_message: Option<String>,

    /// Why the durable document could not be read, when it could not be.
    ///
    /// Set only when the read *failed*; an absent file is an answer (there is
    /// no prior state) and leaves this `None`. While it is set, durable saves
    /// are held so an unreadable document is never replaced by one projected
    /// from whatever little was recovered (issues #541, #445).
    pub durable_read_held: Option<String>,

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
    /// the durable document's `preferences.repository_preferences`. The reducer
    /// reads/writes this in memory; it reaches disk through the durable
    /// projection like every other persisted field.
    pub user_preferences: crate::domain::UserPreferences,

    /// Revision of the durable schema-2 document this state was loaded from
    /// (issue #381). The writer rejects candidates that lost the race, and the
    /// accepted revision is committed back through the persistence completion.
    pub durable_revision: u64,

    /// Highest revision proposed by a staged save (issue #381).
    ///
    /// Candidate revisions must be monotonic even while an earlier save is
    /// still in flight, otherwise two candidates would claim the same revision
    /// and the writer could not tell which one supersedes the other. Tracked
    /// separately from [`Self::durable_revision`], which only advances once a
    /// write is acknowledged.
    pub proposed_revision: u64,

    /// Schema-1 fields retained verbatim by migration because no schema-2
    /// owner claims them (issue #381). Carried through load -> save unchanged
    /// so a future owner can adopt them; never interpreted at runtime.
    pub dormant_records: Vec<crate::domain::DormantRecord>,

    /// GitHub Actions mode state (runtime-only — omitted from persisted DTO).
    pub actions_state: ActionsState,

    /// Errors-mode state (runtime-only — omitted from persisted DTO).
    /// Captures the last N errors for the dedicated errors panel (issue #292).
    pub errors_state: super::ErrorsState,

    /// Settings-shell state (runtime-only — never persisted, issue #387).
    ///
    /// A draft, its theme preview, and the screen's selection all belong to the
    /// session that is looking at them; persisting any of them would mean a
    /// restart could resurrect unsaved work over a file that has moved on.
    pub settings_state: super::SettingsState,

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

    /// Terminal Manager screen state (issue #361 PR B). Runtime-only — never
    /// persisted. Inventory/manager/return state live here.
    pub terminal_manager: super::TerminalManagerState,

    /// Where a focused shell should return when hidden/closed/exited (issue
    /// #361 PR B). Set when entering a shell from the manager so F12/F10/natural
    /// exit restore the manager; Dashboard-entered shells keep Dashboard.
    pub shell_return_target: super::ShellReturnTarget,

    /// Runtime-only cache of dead-agent pane previews (issue #374 S4).
    /// Populated once by the off-lock liveness worker; read by the pure render
    /// projection. Never persisted.
    pub dead_preview: super::DeadAgentPreviewCache,

    /// Bounded pending post-commit effect correlations plus the generation
    /// counters stale completions are validated against (issue #381).
    /// Runtime-only — never persisted; no handle/closure/queue lives here.
    pub pending_effects: super::transition::EffectLedger,

    /// Handle-free provider request reducer state (issue #390 CW-10, Slice B);
    /// runtime-only, never persisted.
    pub provider_requests: super::provider_requests::ProviderRequestState,
}

fn initial_navigation(
    published_workbench: &crate::published_workbench::PublishedWorkbench,
) -> super::navigation::NavState {
    let mut nav = super::navigation::NavState::default();
    if let Some(descriptor) = published_workbench
        .screen_registry()
        .get_identity(nav.screen())
        && nav.ensure_current_relationships(descriptor).is_err()
    {
        std::process::abort();
    }
    nav
}

impl AppState {
    /// Replace navigation with a durable root bound to the committed screen declaration.
    ///
    /// Restoration occurs before the application becomes observable. Exhausting a
    /// process-unique panel identity at this boundary is therefore unrecoverable.
    pub fn restore_navigation_root(&mut self, screen: crate::workbench::ScreenId) {
        let mut nav = super::navigation::NavState::rooted(screen);
        if let Some(descriptor) = self
            .published_workbench
            .screen_registry()
            .get_identity(nav.screen())
            && nav.ensure_current_relationships(descriptor).is_err()
        {
            std::process::abort();
        }
        self.nav = nav;
    }
}

/// Root runtime state bound to one immutable committed workbench.
impl AppState {
    /// Construct empty runtime state bound to the declarations committed for
    /// this process.
    #[must_use]
    pub fn new(
        published_workbench: std::sync::Arc<crate::published_workbench::PublishedWorkbench>,
    ) -> Self {
        Self {
            nav: initial_navigation(&published_workbench),
            published_workbench,
            repositories: Vec::new(),
            agents: Vec::new(),
            available_agent_type_ids: Vec::new(),
            agent_type_availability: Vec::new(),
            agent_probe_generation: 0,
            observations: std::collections::HashMap::new(),
            observation_generations: std::collections::HashMap::new(),
            selected_repository_index: None,
            selected_agent_index: None,
            selected_agent_type_index: 0,
            last_selected_agent_by_repo: Vec::new(),
            pane_focus: PaneFocus::default(),
            terminal_focused: false,
            hide_idle_repositories: false,
            workbench: WorkbenchUiState::default(),
            dashboard_search: DashboardSearchState::default(),
            sticky_visibility: StickyVisibilityState::default(),
            modal: ModalState::default(),
            split_filter: None,
            split_grab_index: None,
            dashboard_grab: None,
            action_availability: None,
            provider_action_health: std::collections::BTreeMap::new(),
            provider_surface_action: None,
            provider_notice: None,
            provider_panels: super::provider_panels::ProviderPanelState::default(),
            resolved_layout: None,
            error_message: None,
            warning_message: None,
            durable_read_held: None,
            issues_state: IssuesState::default(),
            prs_state: PullRequestsState::default(),
            user_preferences: crate::domain::UserPreferences::default(),
            durable_revision: 0,
            proposed_revision: 0,
            dormant_records: Vec::new(),
            actions_state: ActionsState::default(),
            errors_state: super::ErrorsState::default(),
            settings_state: super::SettingsState::default(),
            quit_sequence: QuitSequenceState::default(),
            selection: None,
            selection_dashboard_git_info: None,
            selection_snapshot: None,
            terminal_gesture_state: crate::selection::GestureState::default(),
            help_scroll_offset: 0,
            terminal_history_offset: None,
            terminal_viewport_rows: 0,
            terminal_total_lines: 0,
            override_agent_theme: false,
            transient_queue: TransientAgentQueue::default(),
            shell_overlay: ShellOverlayState::default(),
            terminal_manager: super::TerminalManagerState::default(),
            shell_return_target: super::ShellReturnTarget::default(),
            dead_preview: super::DeadAgentPreviewCache::default(),
            pending_effects: super::transition::EffectLedger::default(),
            provider_requests: super::provider_requests::ProviderRequestState::default(),
        }
    }

    /// The immutable declarations committed for this process.
    #[must_use]
    pub const fn published_workbench(
        &self,
    ) -> &std::sync::Arc<crate::published_workbench::PublishedWorkbench> {
        &self.published_workbench
    }

    /// Apply one declared relationship intent to the active open screen.
    ///
    /// The transition is validated against the immutable published resource
    /// registry and committed only after the pure propagation engine succeeds.
    pub(super) fn apply_relationship_intent(
        &mut self,
        intent: crate::workbench::SourceIntent,
    ) -> Result<Vec<crate::workbench::PortUpdate>, crate::workbench::PropagationAbort> {
        let screen = self.nav.screen();
        let Some(descriptor) = self
            .published_workbench
            .screen_registry()
            .get_identity(screen)
            .cloned()
        else {
            return Err(crate::workbench::PropagationAbort::UnknownScreen { screen });
        };
        let schemas = self.published_workbench.resource_schemas();
        let Some((instance, state)) = self.nav.current_mut().relationship_parts_mut() else {
            return Err(crate::workbench::PropagationAbort::UnknownScreen { screen });
        };
        let transition =
            crate::workbench::propagate(&descriptor, schemas, instance, state, &intent)?;
        *state = transition.state;
        Ok(transition.updates)
    }

    /// The immutable action and binding declarations committed for this process.
    #[must_use]
    pub fn action_registry(&self) -> &crate::domain::action_registry::ActionRegistrySnapshot {
        self.published_workbench.actions()
    }

    /// Latest validated runtime-only availability generation.
    #[must_use]
    pub const fn action_availability_generation(
        &self,
    ) -> Option<&crate::domain::action_registry::AvailabilityGeneration> {
        self.action_availability.as_ref()
    }

    /// Project presentation-ready footer hints from this state's exact
    /// committed workbench identity and latest runtime availability.
    #[must_use]
    pub(crate) fn footer_hints(
        &self,
        input: crate::action_projection::FooterProjectionInput,
    ) -> String {
        crate::action_projection::project_footer_effective(
            self.action_registry(),
            self.action_availability_generation(),
            input,
        )
    }

    /// Project presentation-ready help lines from this state's exact committed
    /// workbench identity and latest runtime availability.
    #[must_use]
    pub(crate) fn help_content_lines(&self) -> Vec<String> {
        crate::action_projection::project_help_content_lines_effective(
            self.action_registry(),
            self.action_availability_generation(),
        )
    }

    /// Borrow Settings declarations and runtime availability from this exact
    /// committed state identity.
    #[must_use]
    pub(crate) fn settings_projection_authority(
        &self,
    ) -> super::settings_view::SettingsProjectionAuthority<'_> {
        super::settings_view::SettingsProjectionAuthority::committed(
            self.published_workbench(),
            self.action_availability_generation(),
        )
    }

    /// Effective availability for one committed action after applying the
    /// latest runtime-only availability generation. Static unavailability is
    /// monotonic and cannot be promoted by runtime health.
    #[must_use]
    pub fn action_availability(
        &self,
        action: &crate::domain::action_registry::ActionId,
    ) -> Option<&crate::domain::action_registry::Availability> {
        self.action_registry()
            .effective_availability_of(self.action_availability.as_ref(), action)
    }

    /// Resolve a chord from committed declarations and apply only the mutable
    /// runtime availability overlay.
    #[must_use]
    pub fn resolve_action(
        &self,
        chord: &crate::domain::keymap::Chord,
        stack: &crate::domain::input_context::ContextStack,
    ) -> crate::domain::action_registry::Resolution {
        use crate::domain::action_registry::{Availability, Resolution};

        match self.action_registry().resolve(chord, stack) {
            Resolution::Dispatch { action, handler } => match self.action_availability(&action) {
                Some(Availability::Unavailable { reason }) => Resolution::Unavailable {
                    action,
                    reason: reason.clone(),
                },
                _ => Resolution::Dispatch { action, handler },
            },
            unavailable @ Resolution::Unavailable { .. } => unavailable,
            other => other,
        }
    }

    /// Construct an explicit aggregate-backed state fixture for unit tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn test_fixture() -> Self {
        Self::new(crate::test_support::published_workbench())
    }
}

pub use super::interaction_types::*;
