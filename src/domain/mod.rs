//! Domain model layer - canonical entity types and invariants.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-002

/// Shared validated target-resolution predicates for remote settings.
pub mod target;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// Actions domain types (workflows, runs, jobs, steps, filters) extracted to
// keep this file under the source-file-size limit.
mod actions;
mod quick_resume;
pub use actions::*;
pub use quick_resume::QuickResume;

// Sandbox engine + platform capability types extracted to keep this file
// under the source-file-size limit.
mod sandbox;
pub use sandbox::*;

/// Pagination contracts (PageToken, ListRequestId) shared across list state
/// and boundary messages. Pure value types, no project-internal deps.
mod pagination;
pub use pagination::*;

// Issues Mode domain entities extracted to keep this file under the
// source-file-size limit.
mod issues;
pub use issues::*;

// Validated GitHub repo reference for issue/PR tracker routing (issue #266).
mod repo_ref;
pub use repo_ref::{GitHubRepoRef, GitHubRepoRefError, GitHubRepoRefErrorReason};

// Typed send-to-agent chooser entry and pure label projection (issue #230).
mod agent_chooser;
pub use agent_chooser::{
    AgentChooserEntry, AgentChooserGitMetadata, ChooserRuntimeConfig, DirtyStatus,
    agent_chooser_label,
};

/// Stable identifier for a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId(pub String);

/// Stable identifier for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

/// Agent runtime used to launch an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    CodePuppy,
    #[default]
    Llxprt,
}

impl AgentKind {
    /// Executable name for this runtime.
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::CodePuppy => "code-puppy",
            Self::Llxprt => "llxprt",
        }
    }

    /// User-facing runtime name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CodePuppy => "code_puppy",
            Self::Llxprt => "LLxprt",
        }
    }

    /// Product display name for user-facing UI labels (e.g. the agent chooser).
    ///
    /// Unlike [`label`](Self::label) (which returns the internal form
    /// identifier), this returns the human-readable product name.
    #[must_use]
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::CodePuppy => "Code Puppy",
            Self::Llxprt => "LLxprt",
        }
    }

    /// Parse a value entered or persisted by a form.
    #[must_use]
    pub fn from_form_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "code_puppy" | "code-puppy" | "codepuppy" => Some(Self::CodePuppy),
            "llxprt" => Some(Self::Llxprt),
            _ => None,
        }
    }

    /// Whether this runtime uses Kennel-mode branding.
    #[must_use]
    pub const fn is_kennel(self) -> bool {
        matches!(self, Self::CodePuppy)
    }
}

/// Check whether a single GitHub owner/repo component contains only valid
/// characters: ASCII alphanumerics, hyphens, underscores, and dots.
///
/// Shared by the clone-identity layer (`app_input::clone_identity`) and the
/// repository form layer (`state::form_build`) so validation cannot drift.
#[must_use]
pub fn is_valid_github_component(component: &str) -> bool {
    component
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Remote SSH execution settings owned by a repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RemoteRepositorySettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub login_user: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub identity_file: PathBuf,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub run_as_user: String,
    #[serde(default)]
    pub setup_env_default: bool,
}

/// A repository is a named codebase container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub name: String,
    pub slug: String,
    pub base_dir: PathBuf,
    pub default_profile: String,
    /// Default Code Puppy model. Empty preserves Code Puppy's own default.
    #[serde(default)]
    pub default_code_puppy_model: String,
    /// GitHub repository in `"owner/repo"` format (e.g. `"acme/widgets"`).
    /// When set, issues mode uses this instead of auto-detecting from git remotes.
    #[serde(default)]
    pub github_repo: String,
    /// Optional override for the GitHub repository that sources issues and PRs
    /// (issue #266). When nonblank, all issue/PR reads and mutations are
    /// routed to this `owner/repo` (e.g. an upstream like
    /// `vybestack/llxprt-jefe`), while cloning, origin checks, dashboard/git
    /// display, and GitHub Actions continue to use [`github_repo`]. Blank
    /// preserves current behavior (issues/PRs sourced from [`github_repo`]).
    /// `#[serde(default)]` keeps existing schema-v1 data compatible.
    #[serde(default)]
    pub github_issue_pr_repo: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    #[serde(default)]
    pub issue_base_prompt: String,
    #[serde(default)]
    pub default_agent_kind: AgentKind,
    pub agent_ids: Vec<AgentId>,
}

// =============================================================================
// Pull Requests Mode domain entities
//
// @plan PLAN-20260624-PR-MODE.P03
// @requirement REQ-PR-006
// @requirement REQ-PR-008
// @requirement REQ-PR-009
// Non-serde transient types mirroring Issue/IssueDetail. Reuses IssueComment
// for PR comments (GitHub PRs are issues for the conversation-comment API).
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-006
/// @requirement REQ-PR-009
/// PR lifecycle state (derived from `gh pr` JSON `state` + `mergedAt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// Merge method for a pull request (mirrors GitHub's three merge types).
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeMethod {
    /// Create a merge commit (`--merge`).
    Merge,
    /// Squash commits into one (`--squash`).
    Squash,
    /// Rebase commits onto base (`--rebase`).
    Rebase,
}

/// All known merge methods in canonical display order.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
pub const MERGE_METHODS: [MergeMethod; 3] =
    [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase];

impl MergeMethod {
    /// User-facing display label (mirrors GitHub's three merge-type buttons).
    ///
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-009
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Merge => "Create a merge commit",
            Self::Squash => "Squash and merge",
            Self::Rebase => "Rebase and merge",
        }
    }

    /// The `gh pr merge` flag for this method.
    ///
    /// @plan PLAN-20260624-PR-MODE.P08
    /// @requirement REQ-PR-009
    /// @pseudocode component-002 lines 115-122
    #[must_use]
    pub const fn gh_flag(self) -> &'static str {
        match self {
            Self::Merge => "--merge",
            Self::Squash => "--squash",
            Self::Rebase => "--rebase",
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 74-101
/// Per-review and aggregate review-decision state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Pending,
    Dismissed,
    ReviewRequired,
    None,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 74-101
/// Per-check and aggregate CI rollup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrCheckStatus {
    Pending,
    Success,
    Failure,
    Neutral,
    None,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 22-34
/// PR list-row entity.
#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: PrState,
    pub author_login: String,
    pub updated_at: String,
    pub head_ref: String,
    pub head_sha: String,
    pub base_ref: String,
    pub is_draft: bool,
    pub review_decision: Option<PrReviewState>,
    pub checks_status: PrCheckStatus,
    pub assignee_summary: String,
    pub labels_summary: String,
    pub comment_count: u64,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 157-165
/// Review summary item (read-only).
#[derive(Debug, Clone)]
pub struct PrReview {
    /// GraphQL node id of this review (`PRR_...`), used to attach review
    /// threads to their parent review. `None` when the API omitted it.
    pub review_id: Option<String>,
    pub author_login: String,
    pub state: PrReviewState,
    pub submitted_at: String,
    pub body: Option<String>,
    /// Line-level review threads attached to this review (issue #119).
    /// Empty when no threads were fetched (graceful degradation).
    pub review_threads: Vec<PrReviewThread>,
}

/// A review-thread conversation group with its line-level comments.
///
/// Each thread carries the GraphQL node id (for resolve/unresolve mutations),
/// its resolved state, the file location it is attached to, and the nested
/// reply comments. Reuses [`IssueComment`] for thread replies so the rendering
/// and message-bus layers share one comment type across the app.
///
/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[derive(Debug, Clone)]
pub struct PrReviewThread {
    /// GraphQL node id used for resolve/unresolve mutations.
    pub thread_id: String,
    /// Whether the thread is currently resolved.
    pub is_resolved: bool,
    /// Whether the thread is outdated (the code it was attached to changed).
    pub is_outdated: bool,
    /// GraphQL node id of the parent review (`PRR_...`) this thread belongs
    /// to, taken from the thread's first comment. `None` when unavailable.
    pub review_id: Option<String>,
    /// File path the thread is attached to (`None` for PR-level threads).
    pub path: Option<String>,
    /// Line number the thread is attached to (`None` for PR-level threads).
    pub line: Option<u32>,
    /// Nested thread reply comments (oldest first).
    pub comments: Vec<IssueComment>,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 174-193
/// CI/check summary item (read-only; `url` is display-only).
#[derive(Debug, Clone)]
pub struct PrCheck {
    pub name: String,
    pub status: PrCheckStatus,
    pub conclusion: String,
    pub url: Option<String>,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
/// @pseudocode component-002 lines 74-101
/// PR detail entity. Reuses [`IssueComment`] for comments.
#[derive(Debug, Clone)]
pub struct PullRequestDetail {
    pub repo_owner_name: String,
    pub number: u64,
    pub title: String,
    pub state: PrState,
    pub is_draft: bool,
    pub author_login: String,
    pub created_at: String,
    pub updated_at: String,
    pub head_ref: String,
    pub head_sha: String,
    pub base_ref: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub body: String,
    pub external_url: String,
    pub review_decision: Option<PrReviewState>,
    pub checks_status: PrCheckStatus,
    pub reviews: Vec<PrReview>,
    pub checks: Vec<PrCheck>,
    pub comments: Vec<IssueComment>,
    pub has_more_comments: bool,
    pub comments_cursor: Option<String>,
    /// Whether the PR can be merged right now (GitHub `mergeable`).
    /// `None` when not yet fetched (e.g. preview-from-list).
    pub mergeable: Option<bool>,
    /// Detailed mergeability status (GitHub `mergeStateStatus`, e.g. "CLEAN",
    /// "BLOCKED", "BEHIND"). `None` when not yet fetched.
    pub merge_state_status: Option<String>,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 259-263
/// PR filter-state choice (Space cycles this on the state field).
/// Default is `Open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PrFilterState {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 264a-264d
/// Review-decision filter choice (issue #20 review signal). `Any` emits no
/// qualifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReviewDecisionFilter {
    #[default]
    Any,
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 264e-264g
/// CI/check-rollup filter choice (issue #20 workflow signal). `Any` emits no
/// qualifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChecksFilter {
    #[default]
    Any,
    Success,
    Failing,
    Pending,
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-258
/// PR filter criteria. Structured fields are AND-composed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrFilter {
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub state: Option<PrFilterState>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub reviewer: String,
    #[serde(default)]
    pub is_draft: Option<bool>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub review_decision: ReviewDecisionFilter,
    #[serde(default)]
    pub checks_status: ChecksFilter,
}

/// Serde default function producing an `IssueFilter` with `state = Open`.
fn default_open_issue_filter() -> IssueFilter {
    IssueFilter {
        state: Some(IssueFilterState::Open),
        ..IssueFilter::default()
    }
}

/// Serde default function producing a `PrFilter` with `state = Open`.
fn default_open_pr_filter() -> PrFilter {
    PrFilter {
        state: Some(PrFilterState::Open),
        ..PrFilter::default()
    }
}

/// Per-repository remembered user preferences (issue #163).
///
/// All remembered selections are scoped per-repository so filter/merge
/// choices made in one repo never leak into another. Persisted as part of
/// `persistence::State` and restored on startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoPreferences {
    /// Last committed issue-list filter (state defaults to Open on first use).
    #[serde(default = "default_open_issue_filter")]
    pub issue_filter: IssueFilter,
    /// Last committed PR-list filter (state defaults to Open on first use).
    #[serde(default = "default_open_pr_filter")]
    pub pr_filter: PrFilter,
    /// Last issue search query text (session+restart persisted).
    #[serde(default)]
    pub issue_search_query: String,
    /// Last PR search query text (session+restart persisted).
    #[serde(default)]
    pub pr_search_query: String,
    /// Last-focused issue filter field index (0-based).
    #[serde(default)]
    pub issue_filter_field_index: usize,
    /// Last-focused PR filter field index (0-based).
    #[serde(default)]
    pub pr_filter_field_index: usize,
    /// Last-selected merge method for the merge chooser (`None` until the user
    /// confirms a merge; the chooser then defaults to Merge).
    #[serde(default)]
    pub last_merge_method: Option<MergeMethod>,
}

impl Default for RepoPreferences {
    fn default() -> Self {
        Self {
            issue_filter: default_open_issue_filter(),
            pr_filter: default_open_pr_filter(),
            issue_search_query: String::new(),
            pr_search_query: String::new(),
            issue_filter_field_index: 0,
            pr_filter_field_index: 0,
            last_merge_method: None,
        }
    }
}

/// Aggregate per-repository user preferences (issue #163).
///
/// Mirrors the `last_selected_agent_by_repo` `Vec<(RepositoryId, _)>` pattern:
/// a small vec keyed by repository id. Methods keep the entry for the
/// current repo in sync with the live Issues/PR state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPreferences {
    #[serde(default)]
    pub by_repo: Vec<(RepositoryId, RepoPreferences)>,
}

impl UserPreferences {
    /// Return the stored preferences for `repo_id`, or the Open-default set if
    /// the repo has no stored entry yet (issue #163).
    #[must_use]
    pub fn for_repo(&self, repo_id: &RepositoryId) -> RepoPreferences {
        self.by_repo
            .iter()
            .find(|(id, _)| id == repo_id)
            .map_or_else(RepoPreferences::default, |(_, prefs)| prefs.clone())
    }

    /// Return only the remembered merge method for `repo_id` (issue #163).
    /// Narrower than `for_repo` so the merge-chooser open path does not clone
    /// the full `RepoPreferences` (with its many `String` filter fields) just
    /// to read a single `Option<MergeMethod>`.
    #[must_use]
    pub fn last_merge_method_for(&self, repo_id: &RepositoryId) -> Option<MergeMethod> {
        self.by_repo
            .iter()
            .find(|(id, _)| id == repo_id)
            .and_then(|(_, prefs)| prefs.last_merge_method)
    }

    /// Upsert preferences for `repo_id`: replace an existing entry or push a
    /// new one.
    pub fn update_for_repo(&mut self, repo_id: &RepositoryId, prefs: RepoPreferences) {
        if let Some(entry) = self.by_repo.iter_mut().find(|(id, _)| id == repo_id) {
            entry.1 = prefs;
        } else {
            self.by_repo.push((repo_id.clone(), prefs));
        }
    }

    /// Mutate a single repo's preferences in place via `f`, inserting a fresh
    /// Open-default entry when the repo has no stored entry yet (issue #163).
    /// Avoids the full clone-and-replace of `for_repo`/`update_for_repo` when
    /// only one field changes (e.g. cursor navigation).
    pub fn update_field_for_repo(
        &mut self,
        repo_id: &RepositoryId,
        f: impl FnOnce(&mut RepoPreferences),
    ) {
        if let Some((_, prefs)) = self.by_repo.iter_mut().find(|(id, _)| id == repo_id) {
            f(prefs);
        } else {
            let mut prefs = RepoPreferences::default();
            f(&mut prefs);
            self.by_repo.push((repo_id.clone(), prefs));
        }
    }

    /// Remove the stored preferences entry for `repo_id`, if any (issue #163).
    /// Called when a repository is deleted so its preferences do not linger
    /// or get restored if the id is ever reused.
    pub fn remove_for_repo(&mut self, repo_id: &RepositoryId) {
        self.by_repo.retain(|(id, _)| id != repo_id);
    }
}

/// Agent lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentStatus {
    #[default]
    Queued,
    Running,
    Completed,
    Errored,
    Waiting,
    Paused,
    Dead,
}

/// An agent is the primary work unit in Jefe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub display_id: String,
    pub repository_id: RepositoryId,
    #[serde(default)]
    pub shortcut_slot: Option<u8>,
    pub name: String,
    pub description: String,
    pub work_dir: PathBuf,
    pub profile: String,
    /// Optional Code Puppy model override. Empty inherits the repository default.
    #[serde(default)]
    pub code_puppy_model: String,
    /// Explicit Code Puppy YOLO choice.
    #[serde(default)]
    pub code_puppy_yolo: Option<bool>,
    /// Resume the latest Code Puppy autosave for the effective work directory.
    #[serde(default)]
    pub code_puppy_quick_resume: bool,
    pub mode_flags: Vec<String>,
    #[serde(default)]
    pub llxprt_debug: String,
    pub pass_continue: bool,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default = "default_sandbox_engine")]
    pub sandbox_engine: SandboxEngine,
    #[serde(default = "default_sandbox_flags")]
    pub sandbox_flags: String,
    #[serde(default)]
    pub agent_kind: AgentKind,
    pub status: AgentStatus,
    pub runtime_binding: Option<RuntimeBinding>,
}

/// Stable identity of one operating-system process instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Platform process creation discriminator. Windows exposes this as the
    /// process creation FILETIME; `None` supports legacy/Unix bindings.
    #[serde(default)]
    pub started_at: Option<u64>,
}

impl ProcessIdentity {
    #[must_use]
    pub const fn new(pid: u32, started_at: u64) -> Self {
        Self {
            pid,
            started_at: Some(started_at),
        }
    }
}

/// Runtime session binding metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBinding {
    pub session_name: String,
    pub launch_signature: LaunchSignature,
    pub attached: bool,
    pub last_seen: Option<u64>,
    /// OS PID of the worker process (`llxprt`), used as a liveness fallback
    /// when the tmux session is gone but the worker is still alive.
    ///
    /// PID-based liveness is a best-effort heuristic: OS PID reuse can in
    /// principle produce a false positive (a recycled PID appearing alive).
    /// The window is narrow because this check only fires when the tmux
    /// session is *recently* gone, so a real crash is far more likely than a
    /// collision with a recycled PID in that interval.
    /// `#[serde(default)]` for backward-compatible loading of older state.json
    /// files that predate this field.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Process-instance identity captured with the PID. Older state files omit
    /// this field and continue through the legacy PID-only migration path.
    #[serde(default)]
    pub process_identity: Option<ProcessIdentity>,
}

/// Launch signature for recreating runtime sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSignature {
    pub work_dir: PathBuf,
    pub profile: String,
    /// Effective Code Puppy model for this launch.
    #[serde(default)]
    pub code_puppy_model: String,
    /// Explicit Code Puppy YOLO value for this launch.
    #[serde(default)]
    pub code_puppy_yolo: Option<bool>,
    /// Resume the latest Code Puppy autosave for the effective work directory.
    #[serde(default)]
    pub code_puppy_quick_resume: bool,
    pub mode_flags: Vec<String>,
    #[serde(default)]
    pub llxprt_debug: String,
    pub pass_continue: bool,
    pub sandbox_enabled: bool,
    pub sandbox_engine: SandboxEngine,
    pub sandbox_flags: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    #[serde(default)]
    pub agent_kind: AgentKind,
}

impl Agent {
    /// Create a new agent with default values.
    ///
    /// This domain constructor defaults to [`AgentStatus::Queued`] and is
    /// intended for simple construction and testing. App-side creation should
    /// go through [`crate::services::create_agent`], which is the canonical path
    /// and sets `Running` (creation immediately triggers launch).
    ///
    /// Invariant: `pass_continue` defaults to true for new agents.
    #[must_use]
    pub fn new(id: AgentId, repository_id: RepositoryId, name: String, work_dir: PathBuf) -> Self {
        Self {
            id: id.clone(),
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name,
            description: String::new(),
            work_dir,

            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: true, // Default per REQ-FUNC-004
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            agent_kind: AgentKind::default(),
            status: AgentStatus::default(),
            runtime_binding: None,
        }
    }

    /// Check if the agent is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status == AgentStatus::Running
    }
}

impl Repository {
    /// Create a new repository.
    #[must_use]
    pub fn new(id: RepositoryId, name: String, slug: String, base_dir: PathBuf) -> Self {
        Self {
            id,
            name,
            slug,
            base_dir,
            default_profile: String::new(),
            default_code_puppy_model: String::new(),
            github_repo: String::new(),
            github_issue_pr_repo: String::new(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            default_agent_kind: AgentKind::default(),
            agent_ids: Vec::new(),
        }
    }

    /// Resolve the effective issue/PR tracker target (issue #266).
    ///
    /// Returns a validated [`GitHubRepoRef`] for the upstream tracker that
    /// issues and PRs should be read from and mutated against. When
    /// [`github_issue_pr_repo`] is nonblank and valid, that override is
    /// returned; otherwise the fallback [`github_repo`] is used. An empty
    /// result (`Ok(None)`) means no tracker is configured.
    ///
    /// A malformed nonblank override returns `Err` so it fails visibly — it is
    /// never silently mutated to the fallback fork identity. This is the
    /// central resolver: every issue/PR read and mutation path must go
    /// through here (not read `github_repo` directly).
    ///
    /// Clone/origin/Actions paths continue to use [`github_repo`] directly and
    /// must **not** call this method.
    ///
    /// [`github_issue_pr_repo`]: Repository::github_issue_pr_repo
    /// [`github_repo`]: Repository::github_repo
    pub fn effective_issue_pr_repo(&self) -> Result<Option<GitHubRepoRef>, GitHubRepoRefError> {
        let override_trimmed = self.github_issue_pr_repo.trim();
        if !override_trimmed.is_empty() {
            return GitHubRepoRef::parse(override_trimmed);
        }
        GitHubRepoRef::parse(&self.github_repo)
    }
}
#[cfg(test)]
mod tests;
