//! Persistence layer - file-based settings and state storage.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-005
//!
//! Pseudocode reference: component-003 lines 01-08
//!
//! Path resolution order:
//! - settings.toml: JEFE_SETTINGS_PATH -> JEFE_CONFIG_DIR/settings.toml -> platform default
//! - state.json: JEFE_STATE_PATH -> JEFE_STATE_DIR/state.json -> platform default

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::{Agent, AgentId, Repository, RepositoryId};

/// Persistence errors.
#[derive(Debug, Clone)]
pub enum PersistenceError {
    IoError(String),
    ParseError(String),
    SerializeError(String),
    SchemaVersionMismatch { expected: u32, found: u32 },
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "IO error: {msg}"),
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
            Self::SerializeError(msg) => write!(f, "serialize error: {msg}"),
            Self::SchemaVersionMismatch { expected, found } => {
                write!(
                    f,
                    "schema version mismatch: expected {expected}, found {found}"
                )
            }
        }
    }
}

impl std::error::Error for PersistenceError {}

/// Settings schema version.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// State schema version.
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// User settings (persisted to settings.toml).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub schema_version: u32,
    pub theme: String,
}

impl Settings {
    #[must_use]
    pub fn default_with_version() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            theme: String::from("green-screen"),
        }
    }
}

/// Operational state (persisted to state.json).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub schema_version: u32,
    pub repositories: Vec<Repository>,
    pub agents: Vec<Agent>,
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
    #[serde(default)]
    pub hide_idle_repositories: bool,
    #[serde(default)]
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,
}

impl State {
    #[must_use]
    pub fn default_with_version() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            repositories: Vec::new(),
            agents: Vec::new(),
            selected_repository_index: None,
            selected_agent_index: None,
            hide_idle_repositories: false,
            last_selected_agent_by_repo: Vec::new(),
        }
    }
}

/// Resolved paths for persistence files.
#[derive(Debug, Clone)]
pub struct PersistencePaths {
    pub settings_path: PathBuf,
    pub state_path: PathBuf,
}

/// Resolve persistence paths according to precedence rules.
///
/// settings.toml:
/// 1. JEFE_SETTINGS_PATH (absolute file path)
/// 2. JEFE_CONFIG_DIR/settings.toml
/// 3. Platform default
///
/// state.json:
/// 1. JEFE_STATE_PATH (absolute file path)
/// 2. JEFE_STATE_DIR/state.json
/// 3. Platform default
#[must_use]
pub fn resolve_paths() -> PersistencePaths {
    let settings_path = resolve_settings_path();
    let state_path = resolve_state_path();
    PersistencePaths {
        settings_path,
        state_path,
    }
}

fn resolve_settings_path() -> PathBuf {
    // 1. JEFE_SETTINGS_PATH
    if let Ok(path) = std::env::var("JEFE_SETTINGS_PATH") {
        return PathBuf::from(path);
    }

    // 2. JEFE_CONFIG_DIR/settings.toml
    if let Ok(dir) = std::env::var("JEFE_CONFIG_DIR") {
        return PathBuf::from(dir).join("settings.toml");
    }

    // 3. Platform default
    platform_default_config_dir().join("settings.toml")
}

fn resolve_state_path() -> PathBuf {
    // 1. JEFE_STATE_PATH
    if let Ok(path) = std::env::var("JEFE_STATE_PATH") {
        return PathBuf::from(path);
    }

    // 2. JEFE_STATE_DIR/state.json
    if let Ok(dir) = std::env::var("JEFE_STATE_DIR") {
        return PathBuf::from(dir).join("state.json");
    }

    // 3. Platform default
    platform_default_state_dir().join("state.json")
}

/// The config directory used for settings.toml (honors JEFE_CONFIG_DIR /
/// JEFE_SETTINGS_PATH, falling back to the platform default).
///
/// Used by callers (e.g. theme loading) that need to locate sibling
/// subdirectories like `themes/`.
#[must_use]
pub fn default_config_dir() -> PathBuf {
    resolve_config_dir_from_env(
        std::env::var("JEFE_CONFIG_DIR").ok(),
        std::env::var("JEFE_SETTINGS_PATH").ok(),
    )
}

/// Pure config-dir resolver from explicit env values (testable without env mutation).
#[must_use]
fn resolve_config_dir_from_env(
    jefe_config_dir: Option<String>,
    jefe_settings_path: Option<String>,
) -> PathBuf {
    if let Some(dir) = jefe_config_dir.filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(path) = jefe_settings_path.filter(|s| !s.is_empty())
        && let Some(parent) = PathBuf::from(path).parent()
        && !parent.as_os_str().is_empty()
    {
        return parent.to_path_buf();
    }
    platform_default_config_dir()
}

/// The default themes directory: `<config_dir>/themes`.
///
/// This is where custom JSON theme files are loaded from.
#[must_use]
pub fn default_themes_dir() -> PathBuf {
    default_config_dir().join("themes")
}

/// Platform-specific config directory.
///
/// - macOS: ~/Library/Application Support/jefe
/// - Linux: ${XDG_CONFIG_HOME:-~/.config}/jefe
/// - Windows: %APPDATA%\jefe
fn platform_default_config_dir() -> PathBuf {
    dirs::config_dir().map_or_else(|| PathBuf::from(".jefe"), |p| p.join("jefe"))
}

/// Platform-specific state directory.
///
/// - macOS: ~/Library/Application Support/jefe
/// - Linux: ${XDG_STATE_HOME:-~/.local/state}/jefe
/// - Windows: %LOCALAPPDATA%\jefe
fn platform_default_state_dir() -> PathBuf {
    // dirs crate doesn't have state_dir, use data_local_dir as fallback
    dirs::data_local_dir().map_or_else(|| PathBuf::from(".jefe"), |p| p.join("jefe"))
}

/// Persistence manager trait.
pub trait PersistenceManager {
    /// Load settings from disk (or defaults if missing/invalid).
    fn load_settings(&self) -> Result<Settings, PersistenceError>;

    /// Load state from disk (or defaults if missing/invalid).
    fn load_state(&self) -> Result<State, PersistenceError>;

    /// Save settings atomically.
    fn save_settings(&self, settings: &Settings) -> Result<(), PersistenceError>;

    /// Save state atomically.
    fn save_state(&self, state: &State) -> Result<(), PersistenceError>;
}

/// Stub implementation of PersistenceManager for testing.
#[derive(Debug, Default)]
pub struct StubPersistenceManager {
    #[allow(dead_code)]
    paths: Option<PersistencePaths>,
}

impl StubPersistenceManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            paths: Some(resolve_paths()),
        }
    }
}

impl PersistenceManager for StubPersistenceManager {
    fn load_settings(&self) -> Result<Settings, PersistenceError> {
        // Stub: returns defaults
        Ok(Settings::default_with_version())
    }

    fn load_state(&self) -> Result<State, PersistenceError> {
        // Stub: returns defaults
        Ok(State::default_with_version())
    }

    fn save_settings(&self, _settings: &Settings) -> Result<(), PersistenceError> {
        // Stub: no-op
        Ok(())
    }

    fn save_state(&self, _state: &State) -> Result<(), PersistenceError> {
        // Stub: no-op
        Ok(())
    }
}

/// Real file-based implementation of PersistenceManager.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P12
/// @requirement REQ-TECH-005
#[derive(Debug)]
pub struct FilePersistenceManager {
    paths: PersistencePaths,
}

impl Default for FilePersistenceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FilePersistenceManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            paths: resolve_paths(),
        }
    }

    /// Create with custom paths (for testing).
    #[must_use]
    pub fn with_paths(paths: PersistencePaths) -> Self {
        Self { paths }
    }

    /// Atomic write: write to temp file, then rename.
    fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), PersistenceError> {
        use std::fs;
        use std::io::Write;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| PersistenceError::IoError(format!("create dir: {e}")))?;
        }

        // Write to temp file
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| PersistenceError::IoError(format!("create temp: {e}")))?;
        file.write_all(content.as_bytes())
            .map_err(|e| PersistenceError::IoError(format!("write temp: {e}")))?;
        file.sync_all()
            .map_err(|e| PersistenceError::IoError(format!("sync temp: {e}")))?;
        drop(file);

        // Atomic rename
        fs::rename(&temp_path, path)
            .map_err(|e| PersistenceError::IoError(format!("rename: {e}")))?;

        Ok(())
    }
}

impl PersistenceManager for FilePersistenceManager {
    fn load_settings(&self) -> Result<Settings, PersistenceError> {
        use std::fs;

        let path = &self.paths.settings_path;

        // If file doesn't exist, return defaults
        if !path.exists() {
            return Ok(Settings::default_with_version());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| PersistenceError::IoError(format!("read settings: {e}")))?;

        let settings: Settings = toml::from_str(&content)
            .map_err(|e| PersistenceError::ParseError(format!("parse settings: {e}")))?;

        // Schema version check
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            // For now, we accept older versions and migrate on save
            // Future: could return SchemaVersionMismatch error
        }

        Ok(settings)
    }

    fn load_state(&self) -> Result<State, PersistenceError> {
        use std::fs;

        let path = &self.paths.state_path;

        // If file doesn't exist, return defaults
        if !path.exists() {
            return Ok(State::default_with_version());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| PersistenceError::IoError(format!("read state: {e}")))?;

        let state: State = serde_json::from_str(&content)
            .map_err(|e| PersistenceError::ParseError(format!("parse state: {e}")))?;

        Ok(state)
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), PersistenceError> {
        let content = toml::to_string_pretty(settings)
            .map_err(|e| PersistenceError::SerializeError(format!("serialize settings: {e}")))?;

        Self::atomic_write(&self.paths.settings_path, &content)
    }

    fn save_state(&self, state: &State) -> Result<(), PersistenceError> {
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| PersistenceError::SerializeError(format!("serialize state: {e}")))?;

        Self::atomic_write(&self.paths.state_path, &content)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn settings_default_has_green_screen_theme() {
        let settings = Settings::default_with_version();
        assert_eq!(settings.theme, "green-screen");
        assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    }

    #[test]
    fn state_default_has_version() {
        let state = State::default_with_version();
        assert_eq!(state.schema_version, STATE_SCHEMA_VERSION);
    }

    #[test]
    fn resolve_paths_returns_valid_paths() {
        let paths = resolve_paths();
        assert!(paths.settings_path.ends_with("settings.toml"));
        assert!(paths.state_path.ends_with("state.json"));
    }

    #[test]
    fn default_themes_dir_ends_with_themes_subdir() {
        let dir = default_themes_dir();
        assert!(dir.ends_with("themes"));
    }

    #[test]
    fn resolve_config_dir_prefers_jefe_config_dir() {
        let dir = resolve_config_dir_from_env(Some("/custom/config".into()), None);
        assert_eq!(dir, PathBuf::from("/custom/config"));
    }

    #[test]
    fn resolve_config_dir_uses_settings_path_parent_when_no_config_dir() {
        let dir = resolve_config_dir_from_env(None, Some("/a/b/settings.toml".into()));
        assert_eq!(dir, PathBuf::from("/a/b"));
    }

    #[test]
    fn resolve_config_dir_ignores_bare_filename_settings_path() {
        // A bare filename with no parent dir should fall back to platform default.
        let dir = resolve_config_dir_from_env(None, Some("settings.toml".into()));
        assert!(dir.ends_with("jefe"));
    }

    #[test]
    fn resolve_config_dir_falls_back_to_platform_default() {
        let dir = resolve_config_dir_from_env(None, None);
        // Should be the platform default (e.g. ends with "jefe").
        assert!(dir.ends_with("jefe"));
    }

    #[test]
    fn resolve_config_dir_ignores_empty_env_values() {
        let dir = resolve_config_dir_from_env(Some(String::new()), Some(String::new()));
        assert!(dir.ends_with("jefe"));
    }

    #[test]
    fn stub_persistence_returns_defaults() {
        let mgr = StubPersistenceManager::new();
        let settings = mgr.load_settings().expect("should load settings");
        assert_eq!(settings.theme, "green-screen");
    }

    #[test]
    fn file_persistence_returns_defaults_when_missing() {
        let temp = std::env::temp_dir().join("jefe_test_missing");
        let paths = PersistencePaths {
            settings_path: temp.join("settings.toml"),
            state_path: temp.join("state.json"),
        };
        let mgr = FilePersistenceManager::with_paths(paths);

        let settings = mgr.load_settings().expect("should load defaults");
        assert_eq!(settings.theme, "green-screen");

        let state = mgr.load_state().expect("should load defaults");
        assert!(state.repositories.is_empty());
    }

    #[test]
    fn file_persistence_roundtrip_settings() {
        let temp = std::env::temp_dir().join("jefe_test_roundtrip_settings");
        let _ = std::fs::remove_dir_all(&temp);
        let paths = PersistencePaths {
            settings_path: temp.join("settings.toml"),
            state_path: temp.join("state.json"),
        };
        let mgr = FilePersistenceManager::with_paths(paths);

        let settings = Settings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            theme: "dracula".into(),
        };

        mgr.save_settings(&settings).expect("should save");
        let loaded = mgr.load_settings().expect("should load");

        assert_eq!(loaded.theme, "dracula");
        assert_eq!(loaded.schema_version, SETTINGS_SCHEMA_VERSION);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn file_persistence_roundtrip_state() {
        let temp = std::env::temp_dir().join("jefe_test_roundtrip_state");
        let _ = std::fs::remove_dir_all(&temp);
        let paths = PersistencePaths {
            settings_path: temp.join("settings.toml"),
            state_path: temp.join("state.json"),
        };
        let mgr = FilePersistenceManager::with_paths(paths);

        let state = State {
            schema_version: STATE_SCHEMA_VERSION,
            repositories: vec![],
            agents: vec![],
            selected_repository_index: Some(2),
            selected_agent_index: None,
            hide_idle_repositories: true,
            last_selected_agent_by_repo: vec![],
        };

        mgr.save_state(&state).expect("should save");
        let loaded = mgr.load_state().expect("should load");

        assert_eq!(loaded.selected_repository_index, Some(2));
        assert!(loaded.hide_idle_repositories);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn file_persistence_atomic_write_creates_parent_dirs() {
        let temp = std::env::temp_dir()
            .join("jefe_test_atomic")
            .join("nested")
            .join("dirs");
        let _ = std::fs::remove_dir_all(temp.parent().unwrap().parent().unwrap());

        let paths = PersistencePaths {
            settings_path: temp.join("settings.toml"),
            state_path: temp.join("state.json"),
        };
        let mgr = FilePersistenceManager::with_paths(paths);

        mgr.save_settings(&Settings::default_with_version())
            .expect("should create dirs and save");

        // Cleanup
        let _ = std::fs::remove_dir_all(temp.parent().unwrap().parent().unwrap());
    }

    /// Test P13-1: State with repo having issue_base_prompt round-trips through JSON serialization.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-012
    #[test]
    fn test_issue_base_prompt_state_round_trip() {
        use crate::domain::{RemoteRepositorySettings, Repository, RepositoryId};
        use std::path::PathBuf;

        let repo = Repository {
            id: RepositoryId("repo-issues".to_string()),
            name: "Issues Repo".to_string(),
            slug: "issues-repo".to_string(),
            base_dir: PathBuf::from("/tmp/issues-repo"),
            default_profile: String::new(),
            github_repo: String::new(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: "Always reproduce the bug first".to_string(),
            agent_ids: vec![],
        };

        let state = State {
            schema_version: STATE_SCHEMA_VERSION,
            repositories: vec![repo],
            agents: vec![],
            selected_repository_index: Some(0),
            selected_agent_index: None,
            hide_idle_repositories: false,
            last_selected_agent_by_repo: vec![],
        };

        let temp = std::env::temp_dir().join("jefe_test_p13_issue_base_prompt_roundtrip");
        let _ = std::fs::remove_dir_all(&temp);
        let paths = PersistencePaths {
            settings_path: temp.join("settings.toml"),
            state_path: temp.join("state.json"),
        };
        let mgr = FilePersistenceManager::with_paths(paths);

        mgr.save_state(&state).expect("should save state");
        let loaded = mgr.load_state().expect("should load state");

        assert_eq!(loaded.repositories.len(), 1);
        assert_eq!(
            loaded.repositories[0].issue_base_prompt,
            "Always reproduce the bug first"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    /// Test P13-2: Deserializing legacy JSON without issue_base_prompt defaults to empty string.
    ///
    /// @plan PLAN-20260329-ISSUES-MODE.P13
    /// @requirement REQ-ISS-012
    #[test]
    fn test_issue_base_prompt_state_backward_compat() {
        // Simulate legacy JSON that predates the issue_base_prompt field
        let legacy_json = serde_json::json!({
            "schema_version": 1,
            "repositories": [
                {
                    "id": "repo-legacy",
                    "name": "Legacy Repo",
                    "slug": "legacy-repo",
                    "base_dir": "/tmp/legacy-repo",
                    "default_profile": "",
                    "agent_ids": []
                    // Note: no issue_base_prompt field
                }
            ],
            "agents": [],
            "selected_repository_index": null,
            "selected_agent_index": null
        });

        let state: State =
            serde_json::from_value(legacy_json).expect("legacy JSON should deserialize");

        assert_eq!(state.repositories.len(), 1);
        // Must default to empty string, not error
        assert_eq!(state.repositories[0].issue_base_prompt, "");
    }
}
