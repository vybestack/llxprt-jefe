//! Domain model layer - canonical entity types and invariants.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-002

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable identifier for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueFilter {
    pub query_text: String,
    pub state: Option<IssueFilterState>,
    pub author: String,
    pub assignee: String,
    pub labels: Vec<String>,
    pub issue_type: String,
    pub milestone: String,
    pub module: String,
    pub mentioned: String,
    pub updated_before: String,
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
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 259-263
/// PR filter-state choice (Space cycles this on the state field).
/// Default is `Open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrFilter {
    pub query_text: String,
    pub state: Option<PrFilterState>,
    pub author: String,
    pub assignee: String,
    pub reviewer: String,
    pub is_draft: Option<bool>,
    pub labels: Vec<String>,
    pub review_decision: ReviewDecisionFilter,
    pub checks_status: ChecksFilter,
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
mod tests {
    use super::*;
    use serde_json::json;

    trait TestResultExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }
    }
    #[test]
    fn issue_filter_default_and_open_state_are_not_active() {
        let mut filter = IssueFilter::default();
        assert!(!filter.has_active_non_default_filters());

        filter.state = Some(IssueFilterState::Open);
        assert!(!filter.has_active_non_default_filters());
    }

    #[test]
    fn issue_filter_closed_all_and_extended_fields_are_active() {
        let mut filter = IssueFilter {
            state: Some(IssueFilterState::Closed),
            ..IssueFilter::default()
        };
        assert!(filter.has_active_non_default_filters());

        filter.state = Some(IssueFilterState::All);
        assert!(filter.has_active_non_default_filters());

        filter.state = None;
        filter.updated_after = "2026-01-01".to_string();
        assert!(filter.has_active_non_default_filters());
    }

    #[test]
    fn issue_filter_any_sentinel_is_not_active_but_none_is_active() {
        let mut filter = IssueFilter {
            author: "any".to_string(),
            assignee: FILTER_CHOICE_ANY.to_string(),
            issue_type: "ANY".to_string(),
            milestone: "ANY".to_string(),
            module: "any".to_string(),
            mentioned: "any".to_string(),
            updated_before: "ANY".to_string(),
            updated_after: "Any".to_string(),
            ..IssueFilter::default()
        };
        assert!(!filter.has_active_non_default_filters());

        filter.query_text = "any".to_string();
        assert!(filter.has_active_non_default_filters());

        filter.query_text.clear();
        filter.assignee = FILTER_CHOICE_NONE.to_string();
        assert!(filter.has_active_non_default_filters());

        filter.assignee.clear();
        filter.milestone = FILTER_CHOICE_NONE.to_string();
        assert!(filter.has_active_non_default_filters());
    }

    #[test]
    fn agent_pass_continue_defaults_true() {
        let agent = Agent::new(
            AgentId("test-1".into()),
            RepositoryId("repo-1".into()),
            "Test Agent".into(),
            PathBuf::from("/tmp/test"),
        );
        assert!(agent.pass_continue);
    }

    #[test]
    fn agent_status_defaults_to_queued() {
        let agent = Agent::new(
            AgentId("test-1".into()),
            RepositoryId("repo-1".into()),
            "Test Agent".into(),
            PathBuf::from("/tmp/test"),
        );
        assert_eq!(agent.status, AgentStatus::Queued);
    }

    #[test]
    fn agent_sandbox_defaults_match_requirement() {
        let agent = Agent::new(
            AgentId("test-1".into()),
            RepositoryId("repo-1".into()),
            "Test Agent".into(),
            PathBuf::from("/tmp/test"),
        );
        assert!(agent.llxprt_debug.is_empty());
        assert!(!agent.sandbox_enabled);
        assert_eq!(agent.sandbox_engine, SandboxEngine::Podman);
        assert_eq!(agent.sandbox_flags, DEFAULT_SANDBOX_FLAGS);
    }

    #[test]
    fn agent_deserializes_missing_llxprt_debug_as_empty() {
        let value = json!({
            "id": "agent-1",
            "display_id": "#1",
            "repository_id": "repo-1",
            "name": "Agent One",
            "description": "",
            "work_dir": "/tmp/agent-1",
            "profile": "",
            "mode_flags": ["--yolo"],
            "pass_continue": true,
            "sandbox_enabled": false,
            "sandbox_engine": "podman",
            "sandbox_flags": DEFAULT_SANDBOX_FLAGS,
            "status": "Queued",
            "runtime_binding": null
        });

        let Ok(agent) = serde_json::from_value::<Agent>(value) else {
            panic!("agent should deserialize");
        };
        assert!(agent.llxprt_debug.is_empty());
    }

    #[test]
    fn launch_signature_deserializes_missing_llxprt_debug_as_empty() {
        let value = json!({
            "work_dir": "/tmp/agent-1",
            "profile": "",
            "mode_flags": ["--yolo"],
            "pass_continue": true,
            "sandbox_enabled": true,
            "sandbox_engine": "podman",
            "sandbox_flags": DEFAULT_SANDBOX_FLAGS
        });

        let Ok(signature) = serde_json::from_value::<LaunchSignature>(value) else {
            panic!("launch signature should deserialize");
        };
        assert!(signature.llxprt_debug.is_empty());
        assert_eq!(signature.remote, RemoteRepositorySettings::default());
    }

    #[test]
    fn repository_deserializes_missing_remote_settings_with_defaults() {
        let value = json!({
            "id": "repo-1",
            "name": "Repo One",
            "slug": "repo-one",
            "base_dir": "/tmp/repo-one",
            "default_profile": "",
            "agent_ids": []
        });

        let Ok(repository) = serde_json::from_value::<Repository>(value) else {
            panic!("repository should deserialize");
        };
        assert_eq!(repository.remote, RemoteRepositorySettings::default());
    }

    #[test]
    fn platform_capabilities_macos_supports_all_engines() {
        let caps = PlatformCapabilities::for_os("macos");
        assert!(caps.is_engine_supported(SandboxEngine::Podman));
        assert!(caps.is_engine_supported(SandboxEngine::Docker));
        assert!(caps.is_engine_supported(SandboxEngine::Seatbelt));
        assert_eq!(caps.supported_engines().len(), 3);
    }

    #[test]
    fn platform_capabilities_linux_excludes_seatbelt() {
        let caps = PlatformCapabilities::for_os("linux");
        assert!(caps.is_engine_supported(SandboxEngine::Podman));
        assert!(caps.is_engine_supported(SandboxEngine::Docker));
        assert!(!caps.is_engine_supported(SandboxEngine::Seatbelt));
        assert_eq!(caps.supported_engines().len(), 2);
    }

    #[test]
    fn platform_capabilities_windows_has_no_supported_engines() {
        let caps = PlatformCapabilities::for_os("windows");
        assert!(!caps.is_engine_supported(SandboxEngine::Podman));
        assert!(!caps.is_engine_supported(SandboxEngine::Docker));
        assert!(!caps.is_engine_supported(SandboxEngine::Seatbelt));
        assert!(caps.supported_engines().is_empty());
    }

    #[test]
    fn normalize_engine_returns_none_when_platform_has_no_supported_engines() {
        let caps = PlatformCapabilities::for_os("windows");
        assert_eq!(caps.normalize_engine(SandboxEngine::Seatbelt), None);
    }

    #[test]
    fn next_for_capabilities_returns_self_when_supported_engines_empty() {
        let caps = PlatformCapabilities::for_os("windows");
        assert_eq!(
            SandboxEngine::Docker.next_for_capabilities(&caps),
            SandboxEngine::Docker
        );
    }

    #[test]
    fn platform_capabilities_normalize_unsupported_engine_to_podman() {
        let caps = PlatformCapabilities::for_os("linux");
        assert_eq!(
            caps.normalize_engine(SandboxEngine::Seatbelt),
            Some(SandboxEngine::Podman)
        );
        assert_eq!(
            caps.normalize_engine(SandboxEngine::Docker),
            Some(SandboxEngine::Docker)
        );
    }

    #[test]
    fn platform_capabilities_normalize_is_noop_on_macos() {
        let caps = PlatformCapabilities::for_os("macos");
        assert_eq!(
            caps.normalize_engine(SandboxEngine::Seatbelt),
            Some(SandboxEngine::Seatbelt)
        );
    }

    #[test]
    fn platform_label_returns_readable_names() {
        assert_eq!(
            PlatformCapabilities::for_os("macos").platform_label(),
            "macOS"
        );
        assert_eq!(
            PlatformCapabilities::for_os("linux").platform_label(),
            "Linux"
        );
        assert_eq!(
            PlatformCapabilities::for_os("windows").platform_label(),
            "Windows"
        );
        assert_eq!(
            PlatformCapabilities::for_os("freebsd").platform_label(),
            "Unknown"
        );
    }

    #[test]
    fn seatbelt_deserialization_still_works_across_platforms() {
        // Seatbelt must always deserialize (for persisted state portability).
        // Platform filtering happens at the capabilities layer, not serde.
        let value = json!({
            "id": "agent-seatbelt",
            "display_id": "#1",
            "repository_id": "repo-1",
            "name": "Seatbelt Agent",
            "description": "",
            "work_dir": "/tmp/sb-agent",
            "profile": "",
            "mode_flags": ["--yolo"],
            "pass_continue": true,
            "sandbox_enabled": true,
            "sandbox_engine": "seatbelt",
            "sandbox_flags": DEFAULT_SANDBOX_FLAGS,
            "status": "Queued",
            "runtime_binding": null
        });
        let Ok(agent) = serde_json::from_value::<Agent>(value) else {
            panic!("agent with seatbelt engine should deserialize");
        };
        assert_eq!(agent.sandbox_engine, SandboxEngine::Seatbelt);
    }

    /// Test 25: issue_base_prompt serializes and deserializes correctly.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-013
    /// @pseudocode component-001 lines 190-195
    #[test]
    fn test_issue_base_prompt_serde_roundtrip() {
        let repo = Repository {
            id: RepositoryId("repo-1".to_string()),
            name: "Test Repo".to_string(),
            slug: "test-repo".to_string(),
            base_dir: PathBuf::from("/tmp/test-repo"),
            default_profile: String::new(),
            github_repo: String::new(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: "Prioritize diagnosis".to_string(),
            agent_ids: vec![],
        };

        let json = serde_json::to_value(&repo).value_or_panic("should serialize");
        let repo2: Repository = serde_json::from_value(json).value_or_panic("should deserialize");

        assert_eq!(repo2.issue_base_prompt, "Prioritize diagnosis");
    }

    /// Test 26: issue_base_prompt backward compatibility with missing field.
    /// @plan PLAN-20260329-ISSUES-MODE.P04
    /// @requirement REQ-ISS-013
    /// @pseudocode component-001 lines 196-200
    #[test]
    fn test_issue_base_prompt_backward_compat() {
        let value = json!({
            "id": "repo-1",
            "name": "Test Repo",
            "slug": "test-repo",
            "base_dir": "/tmp/test-repo",
            "default_profile": "",
            "remote": {
                "enabled": false,
                "login_user": "",
                "host": "",
                "run_as_user": "",
                "setup_env_default": false
            },
            "agent_ids": []
            // Note: no issue_base_prompt field
        });

        let repo: Repository = serde_json::from_value(value).value_or_panic("should deserialize");
        assert_eq!(repo.issue_base_prompt, "");
    }

    /// Regression for issue #121: a persisted `state.json` written before the
    /// `pid` field was added to `RuntimeBinding` must still deserialize, with
    /// `pid` defaulting to `None` (via `#[serde(default)]`).
    #[test]
    fn runtime_binding_deserializes_missing_pid_as_none() {
        let value = json!({
            "session_name": "jefe-agent-1",
            "launch_signature": {
                "work_dir": "/tmp/agent-1",
                "profile": "",
                "mode_flags": [],
                "pass_continue": true,
                "sandbox_enabled": false,
                "sandbox_engine": "podman",
                "sandbox_flags": DEFAULT_SANDBOX_FLAGS
            },
            "attached": false,
            "last_seen": null
            // Note: no pid field
        });

        let binding: RuntimeBinding =
            serde_json::from_value(value).value_or_panic("binding should deserialize");
        assert!(binding.pid.is_none());
    }

    #[test]
    fn runtime_binding_roundtrips_pid_when_present() {
        let binding = RuntimeBinding {
            session_name: "jefe-agent-2".to_string(),
            launch_signature: LaunchSignature {
                work_dir: PathBuf::from("/tmp/agent-2"),
                profile: String::new(),
                mode_flags: vec![],
                llxprt_debug: String::new(),
                pass_continue: true,
                sandbox_enabled: false,
                sandbox_engine: SandboxEngine::Podman,
                sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
                remote: RemoteRepositorySettings::default(),
            },
            attached: false,
            last_seen: None,
            pid: Some(42_000),
        };

        let json = serde_json::to_value(&binding).value_or_panic("should serialize");
        let binding2: RuntimeBinding =
            serde_json::from_value(json).value_or_panic("should deserialize");
        assert_eq!(binding2.pid, Some(42_000));
    }
}
