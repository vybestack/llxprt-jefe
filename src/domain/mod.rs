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
    pub name: String,
    pub description: String,
    pub work_dir: PathBuf,
    pub profile: String,
    pub mode_flags: Vec<String>,
    pub pass_continue: bool,
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
    pub pass_continue: bool,
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
            name,
            description: String::new(),
            work_dir,
            profile: String::new(),
            mode_flags: Vec::new(),
            pass_continue: true, // Default per REQ-FUNC-004
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
}
