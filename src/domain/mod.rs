//! Domain model layer - canonical entity types and invariants.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-002

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable identifier for a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId(pub String);

/// Stable identifier for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

/// Default sandbox resource flags passed to llxprt via SANDBOX_FLAGS.
///
/// Memory is expressed in MiB to avoid unitless podman/crun interpretation issues.
pub const DEFAULT_SANDBOX_FLAGS: &str = "--cpus=2 --memory=12288m --pids-limit=256";

/// Sandbox engine to use when launching llxprt sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxEngine {
    #[default]
    Podman,
    Docker,
    #[serde(alias = "sandbox-exec")]
    Seatbelt,
}

/// All known engine variants in canonical order.
const ALL_ENGINES: [SandboxEngine; 3] = [
    SandboxEngine::Podman,
    SandboxEngine::Docker,
    SandboxEngine::Seatbelt,
];

/// Linux-supported engine variants in canonical order.
const LINUX_ENGINES: [SandboxEngine; 2] = [SandboxEngine::Podman, SandboxEngine::Docker];

impl SandboxEngine {
    /// Convert to llxprt CLI `--sandbox-engine` argument.
    #[must_use]
    pub const fn as_llxprt_arg(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
            Self::Seatbelt => "sandbox-exec",
        }
    }

    /// Parse from user-facing form value.
    #[must_use]
    pub fn from_form_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "podman" => Some(Self::Podman),
            "docker" => Some(Self::Docker),
            "seatbelt" | "sandbox-exec" => Some(Self::Seatbelt),
            _ => None,
        }
    }

    /// User-facing display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Podman => "Podman",
            Self::Docker => "Docker",
            Self::Seatbelt => "Seatbelt",
        }
    }

    /// Cycle to the next *supported* engine for form UX.
    #[must_use]
    pub fn next(self) -> Self {
        self.next_for_capabilities(&PlatformCapabilities::current())
    }

    #[must_use]
    fn next_for_capabilities(self, caps: &PlatformCapabilities) -> Self {
        let supported = caps.supported_engines();
        if supported.is_empty() {
            return self;
        }

        let current_pos = supported.iter().position(|e| *e == self);
        match current_pos {
            Some(pos) => supported[(pos + 1) % supported.len()],
            // Current engine not in supported list — reset to first supported.
            None => supported[0],
        }
    }

    /// Parse a form value and advance to the next supported engine.
    #[must_use]
    pub fn next_from_form_value(value: &str) -> Self {
        Self::from_form_value(value).map_or_else(Self::default, Self::next)
    }
}

/// Runtime platform capabilities — resolves which sandbox engines and features
/// are available on the current OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub os: &'static str,
}

impl PlatformCapabilities {
    /// Detect capabilities for the running platform.
    #[must_use]
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS,
        }
    }

    /// Build capabilities for a specific OS (for testing).
    #[must_use]
    pub fn for_os(os: &'static str) -> Self {
        Self { os }
    }

    /// Engines supported on this platform in display/cycle order.
    #[must_use]
    pub fn supported_engines(&self) -> &'static [SandboxEngine] {
        match self.os {
            "macos" => &ALL_ENGINES,
            "linux" => &LINUX_ENGINES,
            _ => &[],
        }
    }

    /// Whether a specific engine is supported on this platform.
    #[must_use]
    pub fn is_engine_supported(&self, engine: SandboxEngine) -> bool {
        match self.os {
            "macos" => true,
            "linux" => !matches!(engine, SandboxEngine::Seatbelt),
            _ => false,
        }
    }

    /// If `engine` is unsupported, return the first supported fallback.
    ///
    /// Returns `None` when this platform supports no sandbox engines.
    #[must_use]
    pub fn normalize_engine(&self, engine: SandboxEngine) -> Option<SandboxEngine> {
        if self.is_engine_supported(engine) {
            return Some(engine);
        }

        self.supported_engines().first().copied()
    }

    /// Short human-readable platform description for diagnostics.
    #[must_use]
    pub fn platform_label(&self) -> &'static str {
        match self.os {
            "macos" => "macOS",
            "linux" => "Linux",
            "windows" => "Windows",
            _ => "Unknown",
        }
    }
}

fn default_sandbox_engine() -> SandboxEngine {
    SandboxEngine::default()
}

fn default_sandbox_flags() -> String {
    DEFAULT_SANDBOX_FLAGS.to_owned()
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
    /// GitHub repository in `"owner/repo"` format (e.g. `"acme/widgets"`).
    /// When set, issues mode uses this instead of auto-detecting from git remotes.
    #[serde(default)]
    pub github_repo: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    #[serde(default)]
    pub issue_base_prompt: String,
    pub agent_ids: Vec<AgentId>,
}
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 83-96
/// Issue state for list display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueState {
    Open,
    Closed,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-006
/// Issue list representation.
#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    /// GraphQL node id (e.g. `I_kwDO...`); required for `deleteIssue`.
    pub node_id: String,
    pub title: String,
    pub state: IssueState,
    pub author_login: String,
    pub updated_at: String,
    pub assignee_summary: String,
    pub labels_summary: String,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub issue_type: String,
    pub milestone: String,
    pub module: String,
    pub comment_count: u64,
    /// Optional lightweight preview body; list/search fetches may leave this empty
    /// so full body content is loaded through `IssueDetail` instead.
    pub body: String,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-009
/// Full issue detail with comments.
#[derive(Debug, Clone)]
pub struct IssueDetail {
    pub repo_owner_name: String,
    pub number: u64,
    /// GraphQL node id (e.g. `I_kwDO...`); required for `deleteIssue`.
    pub node_id: String,
    pub title: String,
    pub state: IssueState,
    pub author_login: String,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub body: String,
    pub external_url: String,
    pub comments: Vec<IssueComment>,
    pub has_more_comments: bool,
    pub comments_cursor: Option<String>,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-009
/// Single issue comment.
#[derive(Debug, Clone)]
pub struct IssueComment {
    pub comment_id: u64,
    pub author_login: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub body: String,
}

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-008
/// Filter state options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IssueFilterState {
    #[default]
    Open,
    Closed,
    All,
}

pub const FILTER_CHOICE_ANY: &str = "any";
pub const FILTER_CHOICE_NONE: &str = "none";

/// @plan PLAN-20260329-ISSUES-MODE.P03
/// @requirement REQ-ISS-008
/// Issue list filter criteria.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueFilter {
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub state: Option<IssueFilterState>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub milestone: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub mentioned: String,
    #[serde(default)]
    pub updated_before: String,
    #[serde(default)]
    pub updated_after: String,
}

impl IssueFilter {
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-008
    /// @pseudocode component-011 lines 1-9
    #[must_use]
    pub fn has_active_non_default_filters(&self) -> bool {
        matches!(
            self.state,
            Some(IssueFilterState::Closed | IssueFilterState::All)
        ) || !self.query_text.trim().is_empty()
            || sentinel_filter_is_active(&self.author)
            || sentinel_filter_is_active(&self.assignee)
            || !self.labels.is_empty()
            || sentinel_filter_is_active(&self.issue_type)
            || sentinel_filter_is_active(&self.milestone)
            || sentinel_filter_is_active(&self.module)
            || sentinel_filter_is_active(&self.mentioned)
            || sentinel_filter_is_active(&self.updated_before)
            || sentinel_filter_is_active(&self.updated_after)
    }
}

fn sentinel_filter_is_active(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case(FILTER_CHOICE_ANY)
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
/// @pseudocode component-002 lines 74-101
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
/// @pseudocode component-002 lines 74-101
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
/// @pseudocode component-002 lines 74-101
pub const MERGE_METHODS: [MergeMethod; 3] =
    [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase];

impl MergeMethod {
    /// User-facing display label (mirrors GitHub's three merge-type buttons).
    ///
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-009
    /// @pseudocode component-002 lines 74-101
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
    pub status: AgentStatus,
    pub runtime_binding: Option<RuntimeBinding>,
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
}

/// Launch signature for recreating runtime sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSignature {
    pub work_dir: PathBuf,
    pub profile: String,
    pub mode_flags: Vec<String>,
    #[serde(default)]
    pub llxprt_debug: String,
    pub pass_continue: bool,
    pub sandbox_enabled: bool,
    pub sandbox_engine: SandboxEngine,
    pub sandbox_flags: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
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
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: true, // Default per REQ-FUNC-004
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
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
            github_repo: String::new(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            agent_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
