//! Transient agent work-directory cleanup on quit (issue #213).
//!
//! When jefe exits, any transient agent's `work_dir` (created under the
//! repository's `transient_agent_dir`, default `/tmp`) is best-effort
//! removed. A left-behind directory is a minor leak, not a data-loss risk,
//! so errors are logged but never propagated — quit must always succeed.

use jefe::state::AppState;

use tracing::warn;

/// Best-effort removal of all transient agent work directories.
///
/// Iterates the live `AppState.agents` list, removing each transient
/// agent's `work_dir`. A non-empty path that does not exist is silently
/// ignored (`NotFound`); all other errors are logged at `warn`.
///
/// This runs on the synchronous quit path. `std::fs::remove_dir_all` is a
/// blocking syscall — if a transient agent's `work_dir` is on a network
/// filesystem, is very large, or is held open by a still-running agent
/// process, this call may block briefly. A left-behind directory is a minor
/// leak, not a data-loss risk, so errors are logged but never propagated —
/// quit must always succeed.
pub fn cleanup_transient_agent_dirs(state: &AppState) {
    for agent in &state.agents {
        if agent.is_transient()
            && !agent.work_dir.as_os_str().is_empty()
            && let Err(error) = std::fs::remove_dir_all(&agent.work_dir)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(
                work_dir = %agent.work_dir.display(),
                error = %error,
                "failed to remove transient agent directory on quit"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{Agent, AgentId, AgentOrigin, RepositoryId};
    use std::path::PathBuf;

    fn transient_agent(work_dir: &str) -> Agent {
        let mut agent = Agent::new(
            AgentId("t1".to_string()),
            RepositoryId("r1".to_string()),
            "Transient".to_string(),
            PathBuf::from(work_dir),
        );
        agent.origin = AgentOrigin::Transient;
        agent
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "jefe-transient-{label}-{}-{seq}",
            std::process::id()
        ))
    }

    #[test]
    fn cleanup_removes_transient_dir() {
        let temp = unique_temp_dir("cleanup-removes");
        std::fs::create_dir_all(&temp).unwrap_or_else(|e| panic!("create test temp dir: {e}"));
        assert!(temp.exists());

        let mut state = AppState::default();
        state
            .agents
            .push(transient_agent(temp.to_str().unwrap_or("")));

        cleanup_transient_agent_dirs(&state);

        assert!(!temp.exists(), "transient dir should be removed on cleanup");
    }

    #[test]
    fn cleanup_skips_persistent_agents() {
        let temp = unique_temp_dir("cleanup-skips-persistent");
        std::fs::create_dir_all(&temp).unwrap_or_else(|e| panic!("create test temp dir: {e}"));

        let mut state = AppState::default();
        // Default Agent::new produces a persistent agent.
        state.agents.push(Agent::new(
            AgentId("p1".to_string()),
            RepositoryId("r1".to_string()),
            "Persistent".to_string(),
            temp.clone(),
        ));

        cleanup_transient_agent_dirs(&state);

        assert!(
            temp.exists(),
            "persistent agent dir must NOT be removed on cleanup"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn cleanup_ignores_missing_dir() {
        let missing = unique_temp_dir("cleanup-missing");
        assert!(!missing.exists());

        let mut state = AppState::default();
        state
            .agents
            .push(transient_agent(missing.to_str().unwrap_or("")));

        // Should not panic or error on a missing directory.
        cleanup_transient_agent_dirs(&state);
    }

    #[test]
    fn cleanup_skips_empty_work_dir() {
        let mut state = AppState::default();
        let mut agent = transient_agent("");
        agent.work_dir = PathBuf::new();
        state.agents.push(agent);

        // Should not attempt removal of an empty path.
        cleanup_transient_agent_dirs(&state);
    }

    #[test]
    fn cleanup_warns_on_non_notfound_error() {
        // Pointing work_dir at a regular file causes remove_dir_all to fail
        // with a non-NotFound error (NotADirectory on Unix), exercising the
        // warn! path. The function must not panic.
        let temp_file = unique_temp_dir("cleanup-file");
        std::fs::write(&temp_file, b"not a dir")
            .unwrap_or_else(|e| panic!("create test file: {e}"));
        assert!(temp_file.is_file());

        let mut state = AppState::default();
        state
            .agents
            .push(transient_agent(temp_file.to_str().unwrap_or("")));

        cleanup_transient_agent_dirs(&state);

        let _ = std::fs::remove_file(&temp_file);
    }
}
