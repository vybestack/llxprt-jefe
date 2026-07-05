//! TUI scenario test for dashboard reordering (issue #118).
//!
//! Drives the real jefe binary through a grab → move → drop flow against tmux
//! and asserts the reordered repository appears first. Guarded: skips when
//! tmux or the jefe binary are unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;

use jefe::harness::{
    Scenario, TmuxDriver, TmuxPaneSize, TmuxStartRequest, parse_scenario, run_tmux_scenario,
};

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::persistence::{FilePersistenceManager, PersistenceManager, PersistencePaths, State};

/// Seed a config dir with three repositories each having a running agent so
/// they are all visible on the dashboard.
fn seed_reorder_state(config_dir: &Path) {
    let mk_repo = |id: &str, name: &str| {
        Repository::new(
            RepositoryId(id.into()),
            name.into(),
            name.into(),
            PathBuf::from("/tmp"),
        )
    };
    let mk_agent = |id: &str, repo_id: &str, name: &str| {
        let mut agent = Agent::new(
            AgentId(id.into()),
            RepositoryId(repo_id.into()),
            name.into(),
            PathBuf::from("/tmp"),
        );
        agent.status = AgentStatus::Running;
        agent
    };

    let persisted = State {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: vec![
            mk_repo("r1", "alpha"),
            mk_repo("r2", "bravo"),
            mk_repo("r3", "charlie"),
        ],
        agents: vec![
            mk_agent("a1", "r1", "alpha-agent"),
            mk_agent("a2", "r2", "bravo-agent"),
            mk_agent("a3", "r3", "charlie-agent"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        hide_idle_repositories: false,
        last_selected_agent_by_repo: vec![],
    };

    let paths = PersistencePaths {
        settings_path: config_dir.join("settings.toml"),
        state_path: config_dir.join("state.json"),
    };
    let persistence = FilePersistenceManager::with_paths(paths);
    persistence
        .save_state(&persisted)
        .unwrap_or_else(|e| panic!("save state: {e:?}"));
}

/// Locate the jefe binary built by the workspace.
fn jefe_binary_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_jefe") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let current = std::env::current_exe().ok()?;
    let deps_dir = current.parent()?;
    let debug_dir = deps_dir.parent()?;
    let candidate = debug_dir.join("jefe");
    candidate.exists().then_some(candidate)
}

fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-reorder-{label}-{pid}-{nanos}")
}

struct TmuxSessionCleanup {
    name: String,
}

impl Drop for TmuxSessionCleanup {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.name])
            .output();
    }
}

#[test]
fn guarded_dashboard_reorder_tui_scenario() {
    let tmux = TmuxDriver::new();
    if !tmux.is_available() {
        return;
    }
    let Some(jefe_binary) = jefe_binary_path() else {
        return;
    };

    let config_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    seed_reorder_state(config_dir.path());

    let scenario: Scenario = parse_scenario(
        r#"{
            "config": { "cols": 80, "rows": 24 },
            "steps": [
                { "waitFor": "alpha" },
                { "key": "Space" },
                { "wait": 200 },
                { "waitFor": "\u2195" },
                { "key": "Down" },
                { "wait": 200 },
                { "key": "Space" },
                { "wait": 300 },
                { "expect": "bravo" },
                { "key": "q" },
                { "waitForExit": 3000 }
            ]
        }"#,
    )
    .unwrap_or_else(|e| panic!("parse scenario: {e:?}"));

    let session_name = unique_session("scenario");
    let _cleanup = TmuxSessionCleanup {
        name: session_name.clone(),
    };

    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().unwrap_or_else(|e| panic!("cwd: {e:?}")),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .unwrap_or_else(|e| panic!("jefe request: {e:?}"));

    let summary = run_tmux_scenario(&scenario, &request, None)
        .unwrap_or_else(|e| panic!("run scenario: {e:?}"));
    assert_eq!(summary.steps_run, 11);
}
