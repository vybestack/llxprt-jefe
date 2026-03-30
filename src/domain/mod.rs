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
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    pub agent_ids: Vec<AgentId>,
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
            remote: RemoteRepositorySettings::default(),
            agent_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
