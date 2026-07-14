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
    SchemaVersionMismatch {
        expected: u32,
        found: u32,
    },
    /// An explicit configuration directory cannot be used for persistence.
    ///
    /// `path` is the directory supplied to `--config`; `reason` explains why it
    /// is unusable (not a directory, unwritable, etc.). Surfaced fail-fast at
    /// startup so silent data loss cannot occur mid-session.
    InvalidConfigDir {
        path: PathBuf,
        reason: String,
    },
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
            Self::InvalidConfigDir { path, reason } => {
                write!(
                    f,
                    "configuration directory '{}' is unusable: {reason}",
                    path.display()
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
    /// When true, jefe applies its theme fg/bg to the embedded agent CLI's
    /// default (transparent) cells, while leaving the agent's explicit ANSI
    /// styling alone. Off by default (issue #179). Uses `#[serde(default)]`
    /// so old settings.toml files without this field deserialize cleanly.
    #[serde(default)]
    pub override_agent_theme: bool,
}

impl Settings {
    #[must_use]
    pub fn default_with_version() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            theme: String::from("green-screen"),
            override_agent_theme: false,
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
    /// Persisted pane focus ("repositories" | "agents" | "terminal"). Stored
    /// as a string rather than the state-layer `PaneFocus` enum so this module
    /// stays within its `domain/`-only dependency budget. Conversion lives in
    /// the app-shell layer (`app_input`/`app_init`).
    #[serde(default)]
    pub pane_focus: String,
    /// Whether the terminal pane had input focus at last save. Restored on
    /// startup so a focused terminal survives restart (issue #160), clamped to
    /// consistency with `pane_focus` during restore.
    #[serde(default)]
    pub terminal_focused: bool,
    /// Per-repository remembered user preferences (issue #163). Restored on
    /// startup so filter/merge/search selections survive restarts.
    #[serde(default)]
    pub user_preferences: crate::domain::UserPreferences,
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
            pane_focus: String::new(),
            terminal_focused: false,
            user_preferences: crate::domain::UserPreferences::default(),
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

/// Resolve persistence paths rooted at an explicit config directory.
///
/// Both `settings.toml` and `state.json` live directly under `dir`. This is
/// used by the `--config <dir>` runtime argument so multiple instances can run
/// against fully isolated config/state without touching the default paths or
/// environment variable overrides.
#[must_use]
pub fn resolve_paths_from_dir(dir: &std::path::Path) -> PersistencePaths {
    PersistencePaths {
        settings_path: dir.join("settings.toml"),
        state_path: dir.join("state.json"),
    }
}

/// Validate that an explicit config directory can persist `settings.toml` and
/// `state.json`.
///
/// This performs fail-fast startup validation so that a typo (e.g. an
/// unwritable path from `--config="$pwd/.config/jefe"`) produces a clear,
/// actionable error instead of silent apparent data loss after the first
/// session.
///
/// Validation steps:
/// 1. Create the directory (and parents) if it does not already exist.
/// 2. Confirm the path is actually a directory.
/// 3. Reject any persistence target (`settings.toml`/`state.json`) that already
///    exists as a directory or non-regular file, since [`FilePersistenceManager`]
///    cannot atomically write to such a path (its `atomic_write` does
///    `File::create` on a temp sibling then `rename`, which cannot replace a
///    directory with a file). Existing regular files are left untouched.
/// 4. Probe write capability for both `settings.toml` and `state.json` by
///    creating a temporary probe file next to each, then removing it. Any probe
///    file left behind on failure is best-effort cleaned up.
///
/// # Errors
///
/// Returns [`PersistenceError::InvalidConfigDir`] when the directory cannot be
/// created, is not a directory, a persistence target already exists as a
/// non-regular path, or either persistence file cannot be written.
pub fn validate_config_dir(dir: &std::path::Path) -> Result<(), PersistenceError> {
    use std::fs;

    // 1. If the path exists, confirm it is actually a directory. A common
    //    failure mode is `--config` pointing at an existing regular file.
    if dir.exists() {
        let metadata = fs::metadata(dir).map_err(|e| PersistenceError::InvalidConfigDir {
            path: dir.to_path_buf(),
            reason: format!("could not read directory metadata: {e}"),
        })?;
        if !metadata.is_dir() {
            return Err(PersistenceError::InvalidConfigDir {
                path: dir.to_path_buf(),
                reason: "path exists but is not a directory".to_string(),
            });
        }
    } else {
        // 2. Create the directory (and parents) if missing.
        fs::create_dir_all(dir).map_err(|e| PersistenceError::InvalidConfigDir {
            path: dir.to_path_buf(),
            reason: format!("could not create directory: {e}"),
        })?;
    }

    // 3. Validate the actual persistence targets. `resolve_paths_from_dir`
    //    mirrors exactly how `build_persistence` roots the files, so this
    //    proves the real atomic_write targets are usable rather than only the
    //    sibling probe paths.
    let targets = resolve_paths_from_dir(dir);
    validate_persistence_target(dir, &targets.settings_path)?;
    validate_persistence_target(dir, &targets.state_path)?;
    validate_atomic_temp_target(dir, &targets.settings_path.with_extension("tmp"))?;
    validate_atomic_temp_target(dir, &targets.state_path.with_extension("tmp"))?;

    // 4. Probe write capability for both persistence files.
    for file_name in ["settings.toml", "state.json"] {
        probe_write(dir, file_name)?;
    }

    Ok(())
}

/// Validate that a single persistence target path can receive an
/// [`FilePersistenceManager`] atomic write.
///
/// `atomic_write` creates a temp sibling (e.g. `settings.tmp`) and renames it
/// over `target`. That rename can only succeed when `target` either does not
/// exist or is a regular file; if `target` already exists as a directory (or
/// any non-regular file) the rename fails at runtime. Such targets are
/// rejected here, fail-fast, without modifying or removing the user's path.
///
/// `config_dir` is the directory supplied to `--config`; it is used as the
/// `InvalidConfigDir` path so callers always see the config directory they
/// passed, while `reason` names the specific target file that is unusable.
///
/// A missing target is allowed (the file will be created on first save), and a
/// pre-existing regular file is allowed (atomic_write will overwrite it).
///
/// # Errors
///
/// Returns [`PersistenceError::InvalidConfigDir`] when the target exists as a
/// directory or non-regular file, or when its metadata cannot be read.
fn validate_persistence_target(
    config_dir: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), PersistenceError> {
    use std::fs;

    // Only inspect targets that already exist. Missing targets are fine: they
    // will be created on the first atomic_write.
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(PersistenceError::InvalidConfigDir {
                path: config_dir.to_path_buf(),
                reason: format!("could not read target metadata: {e}"),
            });
        }
    };

    if metadata.is_file() {
        // A regular file can be overwritten by atomic_write's rename; allow it
        // without touching it.
        return Ok(());
    }

    // Anything else (directory, symlink to a dir, device, fifo, socket, etc.)
    // blocks atomic_write. Surface a clear, target-specific reason including
    // the file name so the user knows exactly which target is unusable.
    let file_name = target
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("persistence file");
    Err(PersistenceError::InvalidConfigDir {
        path: config_dir.to_path_buf(),
        reason: format!("{file_name} exists but is not a regular file (likely a directory)"),
    })
}

fn validate_atomic_temp_target(
    config_dir: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), PersistenceError> {
    validate_persistence_target(config_dir, target)?;

    match std::fs::OpenOptions::new().write(true).open(target) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            let file_name = target
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("temporary persistence file");
            Err(PersistenceError::InvalidConfigDir {
                path: config_dir.to_path_buf(),
                reason: format!("{file_name} exists but cannot be opened for atomic writes: {e}"),
            })
        }
    }
}

/// Write and remove a temporary probe file under `dir` to confirm writes to
/// `file_name` will succeed during the session.
///
/// # Data-loss safety
///
/// The probe uses a process-unique filename and `create_new(true)` so that it:
/// - Never truncates or overwrites an existing file (the open fails instead).
/// - Never removes a file it did not just create (cleanup only runs on the
///   exact probe path this call created).
fn probe_write(dir: &std::path::Path, file_name: &str) -> Result<(), PersistenceError> {
    use std::fs;
    use std::io::Write;

    // Unique probe filename: avoids colliding with any user file and makes it
    // safe to remove only the file this validation created.
    let probe_name = format!(
        ".{file_name}.jefe-probe-{}-{}",
        std::process::id(),
        probe_counter()
    );
    let probe_path = dir.join(&probe_name);

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    {
        Ok(file) => file,
        Err(e) => {
            return Err(PersistenceError::InvalidConfigDir {
                path: dir.to_path_buf(),
                reason: format!("cannot create {file_name}: {e}"),
            });
        }
    };
    if let Err(e) = file.write_all(b"jefe probe") {
        let _ = fs::remove_file(&probe_path);
        return Err(PersistenceError::InvalidConfigDir {
            path: dir.to_path_buf(),
            reason: format!("cannot write {file_name}: {e}"),
        });
    }
    if let Err(e) = file.sync_all() {
        let _ = fs::remove_file(&probe_path);
        return Err(PersistenceError::InvalidConfigDir {
            path: dir.to_path_buf(),
            reason: format!("cannot sync {file_name}: {e}"),
        });
    }
    drop(file);

    if let Err(e) = fs::remove_file(&probe_path) {
        return Err(PersistenceError::InvalidConfigDir {
            path: dir.to_path_buf(),
            reason: format!("cannot remove probe for {file_name}: {e}"),
        });
    }

    Ok(())
}

/// Monotonic counter for generating unique probe filenames within a process.
fn probe_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
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

/// The config directory used for settings.toml (honors JEFE_SETTINGS_PATH /
/// JEFE_CONFIG_DIR, falling back to the platform default).
///
/// Precedence mirrors `resolve_settings_path()`: JEFE_SETTINGS_PATH's parent
/// takes priority, then JEFE_CONFIG_DIR, then platform default.
///
/// Used by callers (e.g. theme loading) that need to locate sibling
/// subdirectories like `themes/`.
#[must_use]
pub fn default_config_dir() -> PathBuf {
    resolve_config_dir_from_env(
        std::env::var("JEFE_SETTINGS_PATH").ok(),
        std::env::var("JEFE_CONFIG_DIR").ok(),
    )
}

/// Pure config-dir resolver from explicit env values (testable without env mutation).
///
/// Precedence mirrors `resolve_settings_path()`:
/// 1. `jefe_settings_path`'s parent directory
/// 2. `jefe_config_dir`
/// 3. Platform default
#[must_use]
fn resolve_config_dir_from_env(
    jefe_settings_path: Option<String>,
    jefe_config_dir: Option<String>,
) -> PathBuf {
    if let Some(path) = jefe_settings_path.filter(|s| !s.is_empty())
        && let Some(parent) = PathBuf::from(path).parent()
        && !parent.as_os_str().is_empty()
    {
        return parent.to_path_buf();
    }
    if let Some(dir) = jefe_config_dir.filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
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

    /// Return the path where settings are stored.
    fn settings_path(&self) -> PathBuf;
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

    fn settings_path(&self) -> PathBuf {
        self.paths.as_ref().map_or_else(
            || PathBuf::from("settings.toml"),
            |p| p.settings_path.clone(),
        )
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

    /// Borrow the resolved persistence paths (for inspection/diagnostics).
    #[must_use]
    pub fn paths_ref(&self) -> &PersistencePaths {
        &self.paths
    }

    /// Serialize and save settings to a specific path (static helper for
    /// use outside a mutex lock).
    pub fn save_settings_to(
        settings: &Settings,
        path: &std::path::Path,
    ) -> Result<(), PersistenceError> {
        let content = toml::to_string_pretty(settings)
            .map_err(|e| PersistenceError::SerializeError(format!("serialize settings: {e}")))?;
        Self::atomic_write(path, &content)
    }

    /// Save pre-serialized state content to a specific path (static helper
    /// for use by the seed API outside a mutex lock).
    pub(crate) fn save_state_to(
        content: &str,
        path: &std::path::Path,
    ) -> Result<(), PersistenceError> {
        Self::atomic_write(path, content)
    }

    /// Atomic write: write to temp file, then rename.
    pub(crate) fn atomic_write(
        path: &std::path::Path,
        content: &str,
    ) -> Result<(), PersistenceError> {
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

    fn settings_path(&self) -> PathBuf {
        self.paths.settings_path.clone()
    }
}

/// Narrow persistence-owned seeding API for isolated config directories.
pub mod seed;

#[cfg(test)]
mod remote_ssh_tests;

#[cfg(test)]
mod runtime_binding_tests;

#[cfg(test)]
mod tests;
