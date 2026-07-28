//! Domain model layer - canonical entity types and invariants.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-002

/// Transport-neutral observation semantic values (issue #476 J1 slice).
pub mod observation;

/// Pure document wrapping and content-line scroll geometry.
pub mod document_wrap;
mod pr_diff;
pub use pr_diff::*;
/// Shared validated target-resolution predicates for remote settings.
pub mod target;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "config_contract_tests.rs"]
mod config_contract_tests;

mod config_contract;
pub use config_contract::{
    ByteSpan, CanonicalDateTime, CanonicalDecimal, CanonicalSemver, ConfigContractError, Id,
    OwnerCatalog, OwnerDescriptor, OwnerKind, ProvenanceKind, ProvenanceOrigin, SecretRef,
    TypedMap, TypedValue,
};

mod state_contract;
pub use state_contract::{
    AgentDefaults, AgentRecord, DormantRecord, LastKnownRuntime, LaunchSignatureV1,
    LocalRepositoryLocation, Preferences, RemoteRepositoryLocation, RepositoryLocation,
    RepositoryRecord, RuntimeRecord, STATE_SCHEMA_V2, Selection, Sha256Digest, StateContractError,
    StateV2,
};

/// Dependency-free SHA-256 used for durable digests and write fencing.
pub mod sha256;

/// Canonical typed-value and identity helpers shared by durable projection
/// and one-way schema-1 migration.
pub mod canonical_values;

/// Closed post-commit effect contract shared by reducer and root shell.
pub mod effects;
#[cfg(test)]
#[path = "effects_tests.rs"]
mod effects_tests;

// Actions domain types (workflows, runs, jobs, steps, filters) extracted to
// keep this file under the source-file-size limit.
mod actions;
mod quick_resume;
mod transient_agent;
pub use actions::*;
pub use quick_resume::QuickResume;

// Error-log domain types (issue #292).
mod errors;
pub use errors::{ERROR_STORE_CAPACITY, ErrorEntry, ErrorSource};

/// Pagination contracts shared across list state and boundary messages.
// Sandbox engine + platform capability types extracted to keep this file
// under the source-file-size limit.
mod sandbox;
pub use sandbox::*;

/// Pagination contracts (PageToken, ListRequestId) shared across list state
/// and boundary messages. Pure value types, no project-internal deps.
mod pagination;
pub use pagination::*;

/// Generic deterministic pagination state container.
mod paginated_list;
pub use paginated_list::{
    AcceptOutcome, BeginOutcome, LoadCorrelation, PageResult, PaginatedList, ReloadResult,
    ReloadVisibility, RequestIdExhausted,
};

// Issues Mode domain entities extracted to keep this file under the
// source-file-size limit.
mod issues;
pub use issues::*;

// Issue-draft rewrite instruction construction (issue #214).
mod issue_rewrite;
pub use issue_rewrite::build_rewrite_instruction;

// Validated GitHub repo reference for issue/PR tracker routing (issue #266).
mod repo_ref;
pub use repo_ref::{GitHubRepoRef, GitHubRepoRefError, GitHubRepoRefErrorReason};

// Normalized LLxprt npm package selector.
mod llxprt_version;
pub use llxprt_version::{
    CODE_PUPPY_PACKAGE, LATEST, LATEST_NIGHTLY, LLXPRT_NPM_PACKAGE, LaunchSource,
    LlxprtNpmPackageSelector, code_puppy_requires_uvx, code_puppy_uvx_from_spec,
    deserialize_optional_selector, is_latest_nightly_sentinel, is_latest_sentinel,
    is_version_sentinel, llxprt_launch_source,
};

// Typed send-to-agent chooser entry and pure label projection (issue #230).
mod agent_chooser;
pub use agent_chooser::{
    AgentChooserEntry, AgentChooserGitMetadata, ChooserRuntimeConfig, DirtyStatus,
    agent_chooser_label,
};

/// Closed four-agent definition contract (issue #382 CW-02).
pub mod agent_definition;
pub use agent_definition::AgentTypeId;

/// Return a shipped definition type id by its canonical registry position.
#[doc(hidden)]
#[must_use]
pub fn shipped_agent_type(index: usize) -> agent_definition::AgentTypeId {
    agent_definition::AgentDefinition::shipped()
        .get(index)
        .map(|definition| definition.id.clone())
        .unwrap_or_default()
}

/// Stable identifier for a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId(pub String);

/// Stable identifier for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

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
    #[serde(default)]
    pub default_type_id: agent_definition::AgentTypeId,
    #[serde(default)]
    pub default_values: TypedMap,
    pub name: String,
    pub slug: String,
    pub base_dir: PathBuf,
    #[serde(default)]
    pub github_repo: String,
    #[serde(default)]
    pub github_issue_pr_repo: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    #[serde(default)]
    pub issue_base_prompt: String,
    #[serde(default)]
    pub transient_agent_dir: PathBuf,
    #[serde(default)]
    pub transient_max_concurrent: u32,
    pub agent_ids: Vec<AgentId>,
}

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
    /// Whether the PR can be merged without conflicts (issue #314).
    /// `Some(true)` = mergeable, `Some(false)` = conflicting, `None` = unknown
    /// (GraphQL `mergeable` enum `UNKNOWN`, or not yet fetched).
    pub mergeable: Option<bool>,
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
    /// Exact diff-side and range metadata for inline placement.
    pub anchor: Option<PrReviewThreadAnchor>,
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
    pub comments: PaginatedList<IssueComment, CommentDetailIdentity>,
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
/// the durable state document and restored on startup.
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
    /// Last-used milestone in the New Issue dialog (issue #407). Sticky:
    /// restored when the dialog opens, remembered on a successful submit.
    #[serde(default)]
    pub last_new_issue_milestone: Option<String>,
    /// Last-used Projects V2 node ids in the New Issue dialog (issue #407).
    /// Sticky: restored when the dialog opens, remembered on submit.
    #[serde(default)]
    pub last_new_issue_project_ids: Vec<String>,
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
            last_new_issue_milestone: None,
            last_new_issue_project_ids: Vec::new(),
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

/// Agent lifecycle status (`ServerLost`: server vanished, preserves binding, #493).
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
    ServerLost,
}

/// An agent is the primary work unit in Jefe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    #[serde(default)]
    pub type_id: agent_definition::AgentTypeId,
    #[serde(default)]
    pub values: TypedMap,
    pub display_id: String,
    pub repository_id: RepositoryId,
    #[serde(default)]
    pub shortcut_slot: Option<u8>,
    pub name: String,
    pub description: String,
    pub work_dir: PathBuf,
    pub status: AgentStatus,
    pub runtime_binding: Option<RuntimeBinding>,
    /// Runtime-only durable signature used to reject stale restoration.
    #[serde(skip)]
    pub persisted_launch_signature: Option<LaunchSignatureV1>,
    /// Whether this agent is persistent or transient (created on-the-fly,
    /// not persisted, cleaned up on exit).
    #[serde(default)]
    pub origin: AgentOrigin,
}

/// Whether an agent was pre-defined by the user or created transiently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentOrigin {
    #[default]
    Persistent,
    Transient,
}

/// Stable identity of one operating-system process instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Platform process creation discriminator. Windows stores creation
    /// FILETIME, Linux stores `/proc` start ticks, and macOS stores UTC epoch
    /// seconds. `None` supports legacy and unavailable platform evidence.
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
    pub launch_signature: LaunchSignatureV1,
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
    /// Lifecycle generation at binding time. Used to reject stale liveness
    /// results after a restart/rebind (issue #301 Phase 4).
    #[serde(default)]
    pub lifecycle_generation: u64,
    /// Captured worker descendant identities (issue #332). On Windows/psmux the
    /// `pane_pid` captures the launcher, not the real worker; persisting the
    /// resolved worker descendant anchors lets a dead-launcher orphan still be
    /// reaped PID-reuse-safely after the launcher dies. Empty for legacy
    /// state.json and for sessions where no descendants were captured.
    #[serde(default)]
    pub worker_identities: Vec<ProcessIdentity>,
}

/// Generic inputs from application state used to compose one immutable launch plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLaunchRequest {
    pub type_id: agent_definition::AgentTypeId,
    pub values: TypedMap,
    pub work_dir: PathBuf,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    #[serde(default)]
    pub operation: agent_definition::Operation,
}

impl AgentLaunchRequest {
    /// Build a generic resume request from the agent's authoritative typed state.
    #[must_use]
    pub fn for_agent(agent: &Agent, repository: &Repository) -> Self {
        Self {
            type_id: agent.type_id.clone(),
            values: agent.values.clone(),
            work_dir: agent.work_dir.clone(),
            remote: repository.remote.clone(),
            operation: agent_definition::Operation::Resume,
        }
    }
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
    pub fn new(
        id: AgentId,
        repository_id: RepositoryId,
        type_id: agent_definition::AgentTypeId,
        values: TypedMap,
        name: String,
        work_dir: PathBuf,
    ) -> Self {
        Self {
            id: id.clone(),
            type_id,
            values,
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name,
            description: String::new(),
            work_dir,
            status: AgentStatus::default(),
            runtime_binding: None,
            persisted_launch_signature: None,
            origin: AgentOrigin::default(),
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
    pub fn new(
        id: RepositoryId,
        default_type_id: agent_definition::AgentTypeId,
        default_values: TypedMap,
        name: String,
        slug: String,
        base_dir: PathBuf,
    ) -> Self {
        Self {
            id,
            default_type_id,
            default_values,
            name,
            slug,
            base_dir,
            github_repo: String::new(),
            github_issue_pr_repo: String::new(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            transient_agent_dir: PathBuf::new(),
            transient_max_concurrent: 0,
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

    /// Resolve the effective transient agent directory (defaults to the
    /// platform temp directory when empty).
    ///
    /// Transient agents are created on-the-fly under this directory. An empty
    /// `transient_agent_dir` field — the default for existing persisted
    /// repos — falls back to `std::env::temp_dir()` which is cross-platform
    /// (`/tmp` on Linux/macOS, `%TEMP%` on Windows).
    #[must_use]
    pub fn effective_transient_dir(&self) -> PathBuf {
        if self.transient_agent_dir.as_os_str().is_empty() {
            std::env::temp_dir()
        } else {
            self.transient_agent_dir.clone()
        }
    }
}
#[cfg(test)]
mod tests;
