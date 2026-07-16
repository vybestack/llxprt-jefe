//! Transient-agent construction from repository defaults and launch snapshots.

use std::path::PathBuf;

use super::{
    Agent, AgentId, AgentKind, AgentOrigin, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature,
    Repository, RepositoryId, SandboxEngine,
};

impl Agent {
    /// Whether this agent is transient (created on-the-fly, not persisted).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.origin == AgentOrigin::Transient
    }

    /// Create a transient agent from an immutable launch snapshot.
    #[must_use]
    pub fn new_transient_from_signature(
        id: AgentId,
        repository_id: RepositoryId,
        repo: &Repository,
        signature: &LaunchSignature,
    ) -> Self {
        debug_assert!(
            signature
                .work_dir
                .starts_with(repo.effective_transient_dir()),
            "transient agent work_dir must be under the repo's effective_transient_dir"
        );
        Self {
            id: id.clone(),
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name: format!("Transient ({})", repo.name),
            description: String::new(),
            work_dir: signature.work_dir.clone(),
            profile: signature.profile.clone(),
            code_puppy_model: signature.code_puppy_model.clone(),
            code_puppy_version: signature.code_puppy_version.clone(),
            code_puppy_yolo: signature.code_puppy_yolo,
            code_puppy_quick_resume: signature.code_puppy_quick_resume,
            mode_flags: signature.mode_flags.clone(),
            llxprt_debug: signature.llxprt_debug.clone(),
            pass_continue: signature.pass_continue,
            sandbox_enabled: signature.sandbox_enabled,
            sandbox_engine: signature.sandbox_engine,
            sandbox_flags: signature.sandbox_flags.clone(),
            agent_kind: signature.agent_kind,
            status: AgentStatus::Queued,
            runtime_binding: None,
            origin: AgentOrigin::Transient,
            llxprt_version: signature.llxprt_version.clone(),
        }
    }

    /// Create a one-shot transient agent from repository defaults.
    ///
    /// The agent runs under the repository's effective transient directory,
    /// is never persisted, and cannot continue an earlier session.
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
            display_id: id.0.clone(),
            repository_id,
            shortcut_slot: None,
            name: format!("Transient ({})", repo.name),
            description: String::new(),
            work_dir,
            profile: repo.default_profile.clone(),
            code_puppy_model: repo.default_code_puppy_model.clone(),
            code_puppy_version: if repo.default_agent_kind == AgentKind::CodePuppy {
                repo.default_code_puppy_version.trim().to_owned()
            } else {
                String::new()
            },
            code_puppy_yolo: repo.default_code_puppy_yolo,
            code_puppy_quick_resume: false,
            mode_flags: repo.default_llxprt_mode_flags.clone(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            agent_kind: repo.default_agent_kind,
            status: AgentStatus::Queued,
            runtime_binding: None,
            origin: AgentOrigin::Transient,
            llxprt_version: repo.default_llxprt_version.clone(),
        }
    }
}
