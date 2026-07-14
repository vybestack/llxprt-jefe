//! Generic persistence-owned seeding API for isolated config directories.
//!
//! External tooling (e.g. documentation-capture harnesses) may need to write
//! a known-good initial `settings.toml` and `state.json` into an isolated
//! Jefe config directory before launching Jefe against it. Rather than letting
//! each tool define its own Jefe schema DTOs and duplicate atomic-write logic,
//! this module exposes a single typed entry point that the persistence layer
//! owns.
//!
//! ## Boundary
//!
//! This module owns only the transactional staging-commit mechanics and the
//! serialization of caller-supplied [`Settings`] and [`State`] values. It does
//! not construct repository/agent/tutorial policy — that is the tool's
//! responsibility. The caller supplies fully-populated canonical domain
//! types; this module serializes and atomically writes them.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::{AgentId, AgentStatus};
use crate::persistence::{PersistenceError, PersistencePaths, Settings, State};

/// Error returned by state-seeding operations.
#[derive(Debug)]
pub enum SeedError {
    /// A persistence-layer error (serialization, I/O, schema mismatch).
    Persistence(PersistenceError),
    /// The configuration directory path was missing a parent (malformed).
    InvalidConfigDir { path: PathBuf },
    /// The config directory contains an entry and is not fresh.
    /// Seeding is refused to protect existing ordinary configs.
    ConfigNotEmpty { path: PathBuf },
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Persistence(err) => write!(f, "seed persistence error: {err}"),
            Self::InvalidConfigDir { path } => write!(
                f,
                "seed config directory '{}' has no parent directory",
                path.display()
            ),
            Self::ConfigNotEmpty { path } => write!(
                f,
                "config directory '{}' is not empty: refusing to overwrite an existing config",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<PersistenceError> for SeedError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

/// Typed generic request to seed an isolated Jefe config directory with
/// caller-supplied canonical [`Settings`] and [`State`].
///
/// The persistence layer acts only as a serializer/I/O owner: it does not
/// construct repository, agent, or tutorial policy. The caller (tool) owns all
/// domain shape — names, descriptions, profiles, IDs, theme, selection, etc.
///
/// Both `settings` and `state` are written atomically via a staged
/// transactional commit so a failure cannot leave a partially seeded target.
#[derive(Debug, Clone)]
pub struct SeedRequest {
    /// Isolated Jefe config directory (resolved by the caller).
    pub config_dir: PathBuf,
    /// Canonical settings to serialize into `settings.toml`.
    pub settings: Settings,
    /// Canonical state to serialize into `state.json`.
    pub state: State,
}

/// Seed an isolated Jefe config directory with caller-supplied canonical
/// settings and state.
///
/// Writes `settings.toml` and `state.json` using atomic writes with a staged
/// transactional commit.
///
/// **Isolation**: fails closed with [`SeedError::ConfigNotEmpty`] if the
/// config directory contains any entry. Both files are prepared in a sibling
/// staging directory and committed together by directory rename, so a failed
/// write cannot leave a partially seeded target.
///
/// # Errors
///
/// Returns [`SeedError`] if the config directory is malformed, is not empty,
/// or a persistence operation fails.
pub fn seed_isolated_config(request: &SeedRequest) -> Result<(), SeedError> {
    seed_isolated_config_with(request, write_seed_files)
}

fn seed_isolated_config_with<F>(request: &SeedRequest, writer: F) -> Result<(), SeedError>
where
    F: FnOnce(&PersistencePaths, &str, &str) -> Result<(), SeedError>,
{
    let parent = request
        .config_dir
        .parent()
        .ok_or_else(|| SeedError::InvalidConfigDir {
            path: request.config_dir.clone(),
        })?;
    // Validate serialization before touching the filesystem.
    let settings_content = toml::to_string_pretty(&request.settings)
        .map_err(|err| PersistenceError::SerializeError(format!("serialize settings: {err}")))?;
    let state_content = serde_json::to_string_pretty(&request.state)
        .map_err(|err| PersistenceError::SerializeError(format!("serialize state: {err}")))?;

    if existing_seed_matches(&request.config_dir, &settings_content, &state_content) {
        return Ok(());
    }
    ensure_config_dir_empty(&request.config_dir)?;

    fs::create_dir_all(parent).map_err(|err| seed_io_error("create config parent", parent, err))?;
    let staging_dir = create_staging_dir(parent)?;
    let staging_paths = resolve_seed_paths(&staging_dir)?;
    if let Err(err) = writer(&staging_paths, &settings_content, &state_content) {
        remove_staging_dir(&staging_dir);
        return Err(err);
    }
    if let Err(err) = commit_staging_dir(&staging_dir, &request.config_dir) {
        remove_staging_dir(&staging_dir);
        return Err(err);
    }
    Ok(())
}
fn existing_seed_matches(config_dir: &Path, settings_content: &str, state_content: &str) -> bool {
    let Ok(entries) = fs::read_dir(config_dir) else {
        return false;
    };
    let mut names = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    names.sort();
    if names
        != [
            OsString::from("settings.toml"),
            OsString::from("state.json"),
        ]
    {
        return false;
    }
    normalized_toml_matches(&config_dir.join("settings.toml"), settings_content)
        && normalized_json_matches(&config_dir.join("state.json"), state_content)
}

fn read_regular(path: &Path) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn normalized_toml_matches(path: &Path, expected: &str) -> bool {
    read_regular(path).and_then(|content| toml::from_str::<toml::Value>(&content).ok())
        == toml::from_str::<toml::Value>(expected).ok()
}

fn normalized_json_matches(path: &Path, expected: &str) -> bool {
    read_regular(path).and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        == serde_json::from_str::<serde_json::Value>(expected).ok()
}

fn ensure_config_dir_empty(config_dir: &Path) -> Result<(), SeedError> {
    if !config_dir.exists() {
        return Ok(());
    }
    if !config_dir.is_dir() {
        return Err(SeedError::ConfigNotEmpty {
            path: config_dir.to_path_buf(),
        });
    }
    let mut entries = fs::read_dir(config_dir)
        .map_err(|err| seed_io_error("inspect config directory", config_dir, err))?;
    if entries
        .next()
        .transpose()
        .map_err(|err| seed_io_error("inspect config directory entry", config_dir, err))?
        .is_some()
    {
        return Err(SeedError::ConfigNotEmpty {
            path: config_dir.to_path_buf(),
        });
    }
    Ok(())
}

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn create_staging_dir(parent: &Path) -> Result<PathBuf, SeedError> {
    for _ in 0..100 {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".jefe-seed-stage-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(seed_io_error("create seed staging directory", &path, err)),
        }
    }
    Err(SeedError::Persistence(PersistenceError::IoError(
        "unable to allocate a unique seed staging directory".to_string(),
    )))
}

fn commit_staging_dir(staging_dir: &Path, config_dir: &Path) -> Result<(), SeedError> {
    commit_staging_dir_with(staging_dir, config_dir, |source, destination| {
        fs::rename(source, destination)
    })
}

fn commit_staging_dir_with<F>(
    staging_dir: &Path,
    config_dir: &Path,
    mut rename: F,
) -> Result<(), SeedError>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    ensure_config_dir_empty(config_dir)?;
    let parent = config_dir
        .parent()
        .ok_or_else(|| SeedError::InvalidConfigDir {
            path: config_dir.to_path_buf(),
        })?;
    if !config_dir.exists() {
        rename(staging_dir, config_dir)
            .map_err(|err| seed_io_error("commit seeded config directory", config_dir, err))?;
        return sync_seed_parent(parent);
    }

    let backup_dir = unique_backup_path(parent);
    rename(config_dir, &backup_dir)
        .map_err(|err| seed_io_error("preserve empty config directory", config_dir, err))?;
    if let Err(commit_err) = rename(staging_dir, config_dir) {
        return match rename(&backup_dir, config_dir) {
            Ok(()) => Err(seed_io_error(
                "commit seeded config directory",
                config_dir,
                commit_err,
            )),
            Err(restore_err) => Err(SeedError::Persistence(PersistenceError::IoError(format!(
                "commit seeded config directory '{}': {commit_err}; restore preserved empty directory '{}': {restore_err}",
                config_dir.display(),
                backup_dir.display()
            )))),
        };
    }
    fs::remove_dir(&backup_dir)
        .map_err(|err| seed_io_error("remove seed backup directory", &backup_dir, err))?;
    sync_seed_parent(parent)
}

fn unique_backup_path(parent: &Path) -> PathBuf {
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".jefe-seed-backup-{}-{sequence}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn sync_seed_parent(parent: &Path) -> Result<(), SeedError> {
    let directory = fs::File::open(parent)
        .map_err(|err| seed_io_error("open seed parent for sync", parent, err))?;
    directory
        .sync_all()
        .map_err(|err| seed_io_error("sync seed parent", parent, err))
}

#[cfg(not(unix))]
fn sync_seed_parent(parent: &Path) -> Result<(), SeedError> {
    fs::metadata(parent)
        .map(|_| ())
        .map_err(|err| seed_io_error("inspect seed parent", parent, err))
}

fn remove_staging_dir(staging_dir: &Path) {
    let _ = fs::remove_dir_all(staging_dir);
}

fn seed_io_error(action: &str, path: &Path, err: std::io::Error) -> SeedError {
    SeedError::Persistence(PersistenceError::IoError(format!(
        "{action} '{}': {err}",
        path.display()
    )))
}

fn write_seed_files(
    paths: &PersistencePaths,
    settings_content: &str,
    state_content: &str,
) -> Result<(), SeedError> {
    crate::persistence::FilePersistenceManager::atomic_write(
        &paths.settings_path,
        settings_content,
    )?;
    crate::persistence::FilePersistenceManager::save_state_to(state_content, &paths.state_path)?;
    sync_seed_parent(
        paths
            .settings_path
            .parent()
            .ok_or_else(|| SeedError::InvalidConfigDir {
                path: paths.settings_path.clone(),
            })?,
    )?;
    Ok(())
}

/// Read the agent IDs currently persisted in an isolated config directory's
/// `state.json`.
///
/// This is a narrow persistence-owned query for external tooling that needs to
/// discover which agents exist in an isolated config (e.g. a documentation
/// capture harness that must target the run-owned nested agent session). The
/// caller never parses the Jefe schema directly — persistence owns that.
///
/// Returns the agent IDs in manifest order. An empty vector means no agents
/// have been seeded yet (the config exists but contains no agents).
///
/// # Errors
///
/// Returns [`SeedError`] if the config directory is malformed or the state
/// file cannot be read or parsed.
pub fn read_agent_ids(config_dir: &std::path::Path) -> Result<Vec<AgentId>, SeedError> {
    let paths = resolve_seed_paths(config_dir)?;
    if !paths.state_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&paths.state_path).map_err(|err| {
        SeedError::Persistence(PersistenceError::IoError(format!(
            "read state '{}': {err}",
            paths.state_path.display()
        )))
    })?;
    let state: State = serde_json::from_str(&content)
        .map_err(|err| PersistenceError::ParseError(format!("parse state: {err}")))?;
    Ok(state.agents.into_iter().map(|agent| agent.id).collect())
}
/// Read agent IDs whose persisted lifecycle state implies an active runtime
/// session. Queued and terminal agents are deliberately omitted.
pub fn read_active_agent_ids(config_dir: &std::path::Path) -> Result<Vec<AgentId>, SeedError> {
    let paths = resolve_seed_paths(config_dir)?;
    if !paths.state_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&paths.state_path).map_err(|err| {
        SeedError::Persistence(PersistenceError::IoError(format!(
            "read state '{}': {err}",
            paths.state_path.display()
        )))
    })?;
    let state: State = serde_json::from_str(&content)
        .map_err(|err| PersistenceError::ParseError(format!("parse state: {err}")))?;
    Ok(state
        .agents
        .into_iter()
        .filter(|agent| {
            matches!(
                agent.status,
                AgentStatus::Running | AgentStatus::Waiting | AgentStatus::Paused
            )
        })
        .map(|agent| agent.id)
        .collect())
}

fn resolve_seed_paths(config_dir: &std::path::Path) -> Result<PersistencePaths, SeedError> {
    if config_dir.parent().is_none() {
        return Err(SeedError::InvalidConfigDir {
            path: config_dir.to_path_buf(),
        });
    }
    Ok(crate::persistence::resolve_paths_from_dir(config_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Agent, AgentId, AgentKind, AgentStatus, Repository, RepositoryId, SandboxEngine,
    };
    use crate::persistence::{SETTINGS_SCHEMA_VERSION, STATE_SCHEMA_VERSION};

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

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|e| panic!("create temp dir: {e:?}"))
    }

    /// Build a generic [`SeedRequest`] with one repository and one agent so
    /// the generic transactional mechanics can be exercised. The repository
    /// and agent construction here mirrors what a tool would supply — the
    /// root persistence layer never constructs these itself.
    fn sample_request(config_dir: &std::path::Path) -> SeedRequest {
        let repo_id = RepositoryId("fixture-clone".to_string());
        let agent_id = AgentId("tutorial-agent".to_string());
        let work_dir = config_dir.join("fixture-clone");
        let repo = Repository {
            id: repo_id.clone(),
            name: "Test Repository".to_string(),
            slug: "fixture-clone".to_string(),
            base_dir: work_dir.clone(),
            default_profile: "llxprt".to_string(),
            default_code_puppy_model: String::new(),
            github_repo: "fixture/test-repo".to_string(),
            github_issue_pr_repo: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            default_agent_kind: AgentKind::Llxprt,
            agent_ids: vec![agent_id.clone()],
        };
        let agent = Agent {
            id: agent_id.clone(),
            display_id: "TestAgent".to_string(),
            repository_id: repo_id,
            shortcut_slot: None,
            name: "TestAgent".to_string(),
            description: "Test agent for seeding".to_string(),
            work_dir,
            profile: "llxprt".to_string(),
            code_puppy_model: String::new(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::default(),
            sandbox_flags: String::new(),
            agent_kind: AgentKind::Llxprt,
            status: AgentStatus::Queued,
            runtime_binding: None,
        };
        let state = State {
            schema_version: STATE_SCHEMA_VERSION,
            repositories: vec![repo],
            agents: vec![agent],
            selected_repository_index: Some(0),
            selected_agent_index: Some(0),
            hide_idle_repositories: false,
            last_selected_agent_by_repo: Vec::new(),
            pane_focus: String::new(),
            terminal_focused: false,
            user_preferences: crate::domain::UserPreferences::default(),
        };
        SeedRequest {
            config_dir: config_dir.to_path_buf(),
            settings: Settings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                theme: "green-screen".to_string(),
                override_agent_theme: false,
            },
            state,
        }
    }

    #[test]
    fn seed_writes_settings_toml_with_theme() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content = std::fs::read_to_string(config_dir.join("settings.toml"))
            .value_or_panic("read settings");
        assert!(
            content.contains("green-screen"),
            "settings must contain theme: {content}"
        );
    }

    #[test]
    fn seed_writes_state_json_with_repository_and_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content =
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state");
        assert_eq!(state.repositories.len(), 1);
        assert_eq!(state.agents.len(), 1);
    }

    #[test]
    fn seed_preserves_caller_supplied_repository_and_agent_ids() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content =
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state");
        assert_eq!(
            state.repositories[0].id,
            RepositoryId("fixture-clone".to_string())
        );
        assert_eq!(state.agents[0].id, AgentId("tutorial-agent".to_string()));
    }

    #[test]
    fn seed_writes_caller_supplied_agent_kind() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let mut request = sample_request(&config_dir);
        request.state.agents[0].agent_kind = AgentKind::CodePuppy;
        request.state.repositories[0].default_agent_kind = AgentKind::CodePuppy;
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content =
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state");
        assert_eq!(state.agents[0].agent_kind, AgentKind::CodePuppy);
        assert_eq!(
            state.repositories[0].default_agent_kind,
            AgentKind::CodePuppy
        );
    }

    #[test]
    fn seed_writes_empty_state_with_no_repositories_or_agents() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let mut request = sample_request(&config_dir);
        request.state.repositories.clear();
        request.state.agents.clear();
        request.state.selected_repository_index = None;
        request.state.selected_agent_index = None;
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content =
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state");
        assert!(state.repositories.is_empty());
        assert!(state.agents.is_empty());
    }

    #[test]
    fn seed_preserves_caller_supplied_theme_exactly() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let mut request = sample_request(&config_dir);
        request.settings.theme = "custom-dark".to_string();
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content = std::fs::read_to_string(config_dir.join("settings.toml"))
            .value_or_panic("read settings");
        assert!(
            content.contains("custom-dark"),
            "settings must contain caller theme: {content}"
        );
    }

    #[test]
    fn seed_preserves_caller_supplied_names_not_root_defaults() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let content =
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state");
        // The root must NOT inject hardcoded names — the caller supplies them.
        assert_eq!(state.repositories[0].name, "Test Repository");
        assert_eq!(state.agents[0].description, "Test agent for seeding");
    }

    // ── read_agent_ids ───────────────────────────────────────────────────

    #[test]
    fn read_agent_ids_returns_empty_when_no_state_file() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));

        let ids = read_agent_ids(&config_dir).value_or_panic("read on empty config should succeed");
        assert!(ids.is_empty(), "no state file means no agents");
    }

    #[test]
    fn read_agent_ids_returns_empty_when_state_has_no_agents() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        // Write a state.json with no agents.
        let empty_state = State::default_with_version();
        let json = serde_json::to_string(&empty_state).value_or_panic("serialize empty state");
        std::fs::write(config_dir.join("state.json"), json).value_or_panic("write state.json");

        let ids = read_agent_ids(&config_dir).value_or_panic("read should succeed");
        assert!(ids.is_empty());
    }

    #[test]
    fn read_agent_ids_returns_seeded_agent_id() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let ids = read_agent_ids(&config_dir).value_or_panic("read should succeed");
        assert_eq!(ids, vec![AgentId("tutorial-agent".to_string())]);
    }

    #[test]
    fn read_active_agent_ids_omits_queued_seeded_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");

        let ids = read_active_agent_ids(&config_dir).value_or_panic("read active IDs");
        assert!(ids.is_empty(), "queued agents have no runtime session");
    }

    #[test]
    fn read_active_agent_ids_omits_terminal_agent_with_retained_binding() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("seed should succeed");
        let paths = resolve_seed_paths(&config_dir).value_or_panic("resolve paths");
        let content = std::fs::read_to_string(&paths.state_path).value_or_panic("read state");
        let mut state: State = serde_json::from_str(&content).value_or_panic("parse state");
        state.agents[0].status = AgentStatus::Completed;
        state.agents[0].runtime_binding = Some(crate::domain::RuntimeBinding {
            session_name: "jefe-stale".to_string(),
            launch_signature: crate::domain::LaunchSignature {
                work_dir: state.agents[0].work_dir.clone(),
                profile: state.agents[0].profile.clone(),
                code_puppy_model: state.agents[0].code_puppy_model.clone(),
                code_puppy_yolo: state.agents[0].code_puppy_yolo,
                code_puppy_quick_resume: state.agents[0].code_puppy_quick_resume,
                mode_flags: state.agents[0].mode_flags.clone(),
                llxprt_debug: state.agents[0].llxprt_debug.clone(),
                pass_continue: state.agents[0].pass_continue,
                sandbox_enabled: state.agents[0].sandbox_enabled,
                sandbox_engine: state.agents[0].sandbox_engine,
                sandbox_flags: state.agents[0].sandbox_flags.clone(),
                remote: crate::domain::RemoteRepositorySettings::default(),
                agent_kind: state.agents[0].agent_kind,
            },
            attached: false,
            last_seen: None,
            pid: None,
            process_identity: None,
        });
        let json = serde_json::to_string(&state).value_or_panic("serialize state");
        std::fs::write(&paths.state_path, json).value_or_panic("write state");

        let ids = read_active_agent_ids(&config_dir).value_or_panic("read active IDs");
        assert!(ids.is_empty(), "terminal agents cannot own live sessions");
    }

    #[test]
    fn read_agent_ids_returns_error_for_malformed_state() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
        std::fs::write(config_dir.join("state.json"), "not valid json")
            .value_or_panic("write garbage");

        let result = read_agent_ids(&config_dir);
        assert!(
            result.is_err(),
            "malformed state must error, not silently empty"
        );
    }

    // ── Isolation: fail closed on existing config ────────────────────────

    #[test]
    fn seed_fails_closed_on_existing_non_empty_state() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));

        // Write a state.json with an existing repository.
        let mut state = State::default_with_version();
        state.repositories.push(Repository {
            id: RepositoryId("existing".to_string()),
            name: "Existing".to_string(),
            slug: "existing".to_string(),
            base_dir: config_dir.join("existing"),
            default_profile: "llxprt".to_string(),
            default_code_puppy_model: String::new(),
            github_repo: String::new(),
            github_issue_pr_repo: String::new(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            default_agent_kind: AgentKind::Llxprt,
            agent_ids: Vec::new(),
        });
        let json = serde_json::to_string(&state).value_or_panic("serialize existing state");
        std::fs::write(config_dir.join("state.json"), json).value_or_panic("write state.json");

        let request = sample_request(&config_dir);
        let result = seed_isolated_config(&request);
        assert!(
            matches!(result, Err(SeedError::ConfigNotEmpty { .. })),
            "seeding over an existing config must fail closed: {result:?}"
        );
    }

    #[test]
    fn seed_rejects_empty_state_leftover() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let empty_state = State::default_with_version();
        let json = serde_json::to_string(&empty_state).value_or_panic("serialize empty state");
        std::fs::write(config_dir.join("state.json"), &json).value_or_panic("write state.json");

        let result = seed_isolated_config(&sample_request(&config_dir));
        assert!(matches!(result, Err(SeedError::ConfigNotEmpty { .. })));
        assert_eq!(
            std::fs::read_to_string(config_dir.join("state.json")).value_or_panic("read state"),
            json
        );
    }

    #[test]
    fn seed_rejects_settings_only_config_without_modifying_it() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let settings = "theme = 'existing'
";
        std::fs::write(config_dir.join("settings.toml"), settings).value_or_panic("write settings");

        let result = seed_isolated_config(&sample_request(&config_dir));
        assert!(matches!(result, Err(SeedError::ConfigNotEmpty { .. })));
        assert_eq!(
            std::fs::read_to_string(config_dir.join("settings.toml"))
                .value_or_panic("read settings"),
            settings
        );
    }

    #[test]
    fn seed_rejects_unrelated_file() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        std::fs::write(config_dir.join("notes.txt"), "keep me").value_or_panic("write unrelated");

        let result = seed_isolated_config(&sample_request(&config_dir));
        assert!(matches!(result, Err(SeedError::ConfigNotEmpty { .. })));
        assert_eq!(
            std::fs::read_to_string(config_dir.join("notes.txt")).value_or_panic("read unrelated"),
            "keep me"
        );
    }

    #[test]
    fn seed_succeeds_when_target_is_absent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        seed_isolated_config(&sample_request(&config_dir)).value_or_panic("seed absent target");
        assert!(config_dir.join("settings.toml").is_file());
        assert!(config_dir.join("state.json").is_file());
    }
    #[test]
    fn identical_seed_is_idempotent_but_mismatch_remains_fail_closed() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        let request = sample_request(&config_dir);
        seed_isolated_config(&request).value_or_panic("initial seed");
        let settings_content = toml::to_string_pretty(&request.settings).value_or_panic("settings");
        let state_content = serde_json::to_string_pretty(&request.state).value_or_panic("state");
        assert!(normalized_toml_matches(
            &config_dir.join("settings.toml"),
            &settings_content
        ));
        assert!(normalized_json_matches(
            &config_dir.join("state.json"),
            &state_content
        ));
        let mut names = std::fs::read_dir(&config_dir)
            .value_or_panic("read config")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            [
                OsString::from("settings.toml"),
                OsString::from("state.json")
            ]
        );
        seed_isolated_config(&request).value_or_panic("identical retry");

        let mut changed = sample_request(&config_dir);
        changed.settings.theme = "different-theme".to_string();
        let result = seed_isolated_config(&changed);
        assert!(matches!(result, Err(SeedError::ConfigNotEmpty { .. })));
    }

    #[test]
    fn writer_failure_preserves_empty_target_and_cleans_staging() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let result =
            seed_isolated_config_with(&sample_request(&config_dir), |paths, settings, _| {
                crate::persistence::FilePersistenceManager::atomic_write(
                    &paths.settings_path,
                    settings,
                )?;
                Err(SeedError::Persistence(PersistenceError::IoError(
                    "injected state write failure".to_string(),
                )))
            });
        assert!(result.is_err());
        assert!(config_dir.is_dir());
        assert_eq!(
            std::fs::read_dir(&config_dir)
                .value_or_panic("read config")
                .count(),
            0
        );
        let staging_entries = std::fs::read_dir(dir.path())
            .value_or_panic("read parent")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".jefe-seed-stage-")
            })
            .count();
        assert_eq!(staging_entries, 0);
    }

    #[test]
    fn commit_failure_restores_existing_empty_target() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        let staging_dir = dir.path().join("staging");
        std::fs::create_dir(&config_dir).value_or_panic("create empty target");
        std::fs::create_dir(&staging_dir).value_or_panic("create staging");
        std::fs::write(staging_dir.join("state.json"), "prepared")
            .value_or_panic("write staging state");
        let mut calls = 0;

        let result = commit_staging_dir_with(&staging_dir, &config_dir, |source, destination| {
            calls += 1;
            if calls == 2 {
                Err(std::io::Error::other("injected commit failure"))
            } else {
                std::fs::rename(source, destination)
            }
        });

        assert!(result.is_err());
        assert!(config_dir.is_dir());
        assert_eq!(
            std::fs::read_dir(&config_dir)
                .value_or_panic("read restored target")
                .count(),
            0
        );
        assert!(staging_dir.join("state.json").is_file());
        let backup_count = std::fs::read_dir(dir.path())
            .value_or_panic("read parent")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".jefe-seed-backup-")
            })
            .count();
        assert_eq!(backup_count, 0);
    }

    #[test]
    fn seed_rejects_config_dir_without_parent() {
        let request = SeedRequest {
            config_dir: std::path::PathBuf::from("/"),
            settings: Settings::default_with_version(),
            state: State::default_with_version(),
        };
        let result = seed_isolated_config(&request);
        assert!(matches!(result, Err(SeedError::InvalidConfigDir { .. })));
    }
}
