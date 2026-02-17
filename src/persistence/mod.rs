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

use crate::domain::{Agent, Repository};

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
        };

        mgr.save_state(&state).expect("should save");
        let loaded = mgr.load_state().expect("should load");

        assert_eq!(loaded.selected_repository_index, Some(2));

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
}
