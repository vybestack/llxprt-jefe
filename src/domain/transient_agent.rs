//! Transient-agent construction from repository defaults and launch requests.

use std::path::PathBuf;

use super::{
    Agent, AgentId, AgentLaunchRequest, AgentOrigin, AgentStatus, Repository, RepositoryId,
};

impl Agent {
    /// Whether this agent is transient (created on-the-fly, not persisted).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.origin == AgentOrigin::Transient
    }

    /// Create a transient agent from a generic launch request.
    #[must_use]
    pub fn new_transient_from_signature(
        id: AgentId,
        repository_id: RepositoryId,
        repo: &Repository,
        request: &AgentLaunchRequest,
    ) -> Self {
        debug_assert!(
            request.work_dir.starts_with(repo.effective_transient_dir()),
            "transient agent work_dir must be under the repo's effective_transient_dir"
        );
        Self {
            id: id.clone(),
            type_id: request.type_id.clone(),
            values: request.values.clone(),
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name: format!("Transient ({})", repo.name),
            description: String::new(),
            work_dir: request.work_dir.clone(),
            status: AgentStatus::Queued,
            runtime_binding: None,
            persisted_launch_signature: None,
            origin: AgentOrigin::Transient,
        }
    }

    /// Create a one-shot transient agent from repository defaults.
    #[must_use]
    pub fn new_transient(
        id: AgentId,
        repository_id: RepositoryId,
        work_dir: PathBuf,
        repo: &Repository,
    ) -> Self {
        debug_assert!(
            work_dir.starts_with(repo.effective_transient_dir()),
            "transient agent work_dir must be under the repo's effective_transient_dir"
        );
        Self {
            id: id.clone(),
            type_id: repo.default_type_id.clone(),
            values: repo.default_values.clone(),
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name: format!("Transient ({})", repo.name),
            description: String::new(),
            work_dir,
            status: AgentStatus::Queued,
            runtime_binding: None,
            persisted_launch_signature: None,
            origin: AgentOrigin::Transient,
        }
    }
}
