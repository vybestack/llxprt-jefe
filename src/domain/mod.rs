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
/// Memory is expressed as a unitless MiB-equivalent integer for sandbox engines.
pub const DEFAULT_SANDBOX_FLAGS: &str = "--cpus=2 --memory=12288 --pids-limit=256";

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
            Self::Podman => "podman",
            Self::Docker => "docker",
            Self::Seatbelt => "seatbelt",
        }
    }

    /// Cycle to the next engine for form UX.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Podman => Self::Docker,
            Self::Docker => Self::Seatbelt,
            Self::Seatbelt => Self::Podman,
        }
    }
}

fn default_sandbox_engine() -> SandboxEngine {
    SandboxEngine::default()
}

fn default_sandbox_flags() -> String {
    DEFAULT_SANDBOX_FLAGS.to_owned()
}

/// A repository is a named codebase container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub name: String,
    pub slug: String,
    pub base_dir: PathBuf,
    pub default_profile: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    }
}
