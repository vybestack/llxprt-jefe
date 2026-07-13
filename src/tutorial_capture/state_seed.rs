//! Typed Jefe state seeding for Tier-B capture runs.
//!
//! Tier-B capture needs Jefe to start with the cloned fixture repository
//! already registered, the GitHub repo association set, and an agent
//! available so the scenario can immediately exercise Issues/PRs/send-to-agent
//! flows. These are **setup-boundary** writes to the isolated config
//! directory — the actions being *documented* (browsing issues, sending to
//! agent, merge chooser) are still driven through the real Jefe UI during
//! the capture scenario.
//!
//! ## What this seeds (setup, not documented actions)
//!
//! - `settings.toml`: theme matching the run manifest.
//! - `state.json`: one `Repository` pointing at the `fixture-clone` path with
//!   the fixture `github_repo` association, plus one `Agent` bound to that
//!   repository so the terminal/send-to-agent flows have a target.
//!
//! ## What this does NOT seed
//!
//! - It does not create issues, branches, or PRs (that is the executor's job).
//! - It does not merge (merge belongs in the capture scenario).
//! - It does not drive any Jefe UI interaction.
//!
//! ## Boundary
//!
//! This module owns writing the isolated Jefe config files (settings.toml,
//! state.json) for a Tier-B run. It delegates path resolution to the main
//! `persistence` module's `resolve_paths_from_dir`.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::path::{Path, PathBuf};

use crate::domain::{
    Agent, AgentId, AgentKind, AgentStatus, Repository, RepositoryId, SandboxEngine,
};
use crate::persistence::{
    self, PersistencePaths, SETTINGS_SCHEMA_VERSION, STATE_SCHEMA_VERSION, State,
};
// AgentKind also needs Default for the repository's default_agent_kind field.

use super::manifest::RuntimeProfile;
use super::path_shim::ShimAvailability;

/// Error returned by state seeding operations.
#[derive(Debug)]
pub enum StateSeedError {
    /// A filesystem operation failed.
    Io { path: PathBuf, reason: String },
    /// Serialization of settings or state failed.
    Serialize { reason: String },
}

impl std::fmt::Display for StateSeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, reason } => {
                write!(f, "state seed I/O error at '{}': {reason}", path.display())
            }
            Self::Serialize { reason } => {
                write!(f, "state seed serialization error: {reason}")
            }
        }
    }
}

impl std::error::Error for StateSeedError {}

impl From<std::io::Error> for StateSeedError {
    fn from(value: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            reason: value.to_string(),
        }
    }
}

/// Configuration for seeding Jefe state for a Tier-B run.
///
/// All fields are required so the seeded state is complete and the scenario
/// can immediately exercise Issues/PRs/send-to-agent without manual setup.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone)]
pub struct TierBStateSeed {
    /// Isolated Jefe config directory (from `prepare`).
    pub config_dir: PathBuf,
    /// Path to the cloned fixture repository (fixture-clone).
    pub fixture_clone_path: PathBuf,
    /// GitHub `owner/repo` for the fixture repository.
    pub fixture_github_repo: String,
    /// Theme name for settings.toml.
    pub theme: String,
    /// Agent name for the seeded agent.
    pub agent_name: String,
    /// Agent kind to seed (derived from runtime profile / shim availability).
    ///
    /// **Finding #4**: Derived from manifest shim availability/runtime profile
    /// rather than hardcoded to Llxprt. Supports LLxprt and Code Puppy paths.
    pub agent_kind: AgentKind,
}

/// The result of seeding Jefe state: the repository ID and agent ID that were
/// written, so the capture scenario can reference them if needed.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededState {
    pub repository_id: RepositoryId,
    pub agent_id: AgentId,
}

/// The stable repository ID used for the seeded fixture repository.
const SEEDED_REPO_ID: &str = "fixture-clone";

/// The stable agent ID used for the seeded agent.
const SEEDED_AGENT_ID: &str = "tutorial-agent";

/// The fixed theme for tutorial-capture runs: Green Screen.
///
/// **Finding #6**: Tutorial-capture always uses Green Screen for consistency
/// and reproducibility. The ANSI SVG renderer derives its default/reset
/// palette from this theme's colors.
const GREEN_SCREEN_THEME: &str = "green-screen";

/// Derive the agent kind from the runtime profile and shim availability.
///
/// **Finding #4**: Instead of hardcoding `AgentKind::Llxprt`, this function
/// derives the seed agent kind from the manifest's runtime profile and shim
/// availability:
/// - `RealLlxprt` → `Llxprt`
/// - `RealCodePuppy` → `CodePuppy`
/// - `Shim` + `LlxprtOnly` → `Llxprt`
/// - `Shim` + `CodePuppyOnly` → `CodePuppy`
/// - `Shim` + `Both` → `Llxprt` (default: when both are available, prefer Llxprt)
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn derive_agent_kind(profile: RuntimeProfile, availability: ShimAvailability) -> AgentKind {
    match profile {
        RuntimeProfile::RealLlxprt => AgentKind::Llxprt,
        RuntimeProfile::RealCodePuppy => AgentKind::CodePuppy,
        RuntimeProfile::Shim => {
            if availability.includes_code_puppy() && !availability.includes_llxprt() {
                AgentKind::CodePuppy
            } else {
                AgentKind::Llxprt
            }
        }
    }
}

/// Seed the isolated Jefe config directory with a repository (pointing at the
/// fixture clone), the GitHub repo association, and an agent.
///
/// This writes:
/// - `settings.toml` with the given theme.
/// - `state.json` with one repository and one agent.
///
/// The repository's `base_dir` is set to `fixture_clone_path` so Jefe uses the
/// cloned fixture repo as its working directory. The `github_repo` field is set
/// so Issues/PRs modes use the fixture repo without auto-detecting from git
/// remotes.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`StateSeedError`] if writing settings.toml or state.json fails.
pub fn seed_tier_b_state(seed: &TierBStateSeed) -> Result<SeededState, StateSeedError> {
    let paths = persistence::resolve_paths_from_dir(&seed.config_dir);
    write_settings(&paths, &seed.theme)?;
    let seeded = build_seeded_state(seed);
    write_state(&paths, &seeded)?;
    Ok(SeededState {
        repository_id: RepositoryId(SEEDED_REPO_ID.to_string()),
        agent_id: AgentId(SEEDED_AGENT_ID.to_string()),
    })
}

/// Serialized settings representation for the seeded config.
#[derive(serde::Serialize)]
struct SettingsOut {
    schema_version: u32,
    theme: String,
    #[serde(default)]
    override_agent_theme: bool,
}

/// Write settings.toml with the given theme and current schema version.
///
/// Uses the Green Screen theme as the fixed tutorial-capture theme
/// (Finding #6).
///
/// **Finding**: Uses atomic write (unique exclusive temp + fsync + rename +
/// dir sync) so a crash never leaves a partially-written settings.toml.
fn write_settings(paths: &PersistencePaths, theme: &str) -> Result<(), StateSeedError> {
    let effective_theme = if theme.is_empty() {
        GREEN_SCREEN_THEME
    } else {
        theme
    };
    let settings = SettingsOut {
        schema_version: SETTINGS_SCHEMA_VERSION,
        theme: effective_theme.to_string(),
        override_agent_theme: false,
    };
    let toml_str = toml::to_string_pretty(&settings).map_err(|e| StateSeedError::Serialize {
        reason: e.to_string(),
    })?;
    atomic_write_file(&paths.settings_path, &toml_str)?;
    Ok(())
}

/// Write state.json with the seeded repository and agent.
fn write_state(paths: &PersistencePaths, state: &State) -> Result<(), StateSeedError> {
    let json = serde_json::to_string_pretty(state).map_err(|e| StateSeedError::Serialize {
        reason: e.to_string(),
    })?;
    atomic_write_file(&paths.state_path, &json)?;
    Ok(())
}

/// Atomically write a file: create a unique exclusive temp file, write
/// content, fsync, rename, then fsync the parent directory. This prevents
/// partial writes from corrupting settings.toml or state.json on crash.
///
/// **Finding**: Uses `create_new` to reject existing symlinks/temp files,
/// a unique temp name (PID + counter + timestamp) to avoid concurrent
/// write collisions, and fsync of both file and parent directory for
/// crash durability.
fn atomic_write_file(path: &Path, content: &str) -> Result<(), StateSeedError> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("state");
    let tmp_name = format!(".{base}.{pid}.{time}.{seq}.tmp");
    let tmp = parent.join(&tmp_name);

    // Use create_new so if a symlink or file already exists at the temp path,
    // we get an error instead of following a symlink.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| StateSeedError::Io {
            path: tmp.clone(),
            reason: e.to_string(),
        })?;
    file.write_all(content.as_bytes())
        .map_err(|e| StateSeedError::Io {
            path: tmp.clone(),
            reason: e.to_string(),
        })?;
    file.sync_all().map_err(|e| StateSeedError::Io {
        path: tmp.clone(),
        reason: e.to_string(),
    })?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|e| StateSeedError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    // fsync the parent directory so the rename is durable.
    fsync_dir(parent);
    Ok(())
}

/// fsync a directory file descriptor for durability of rename operations.
#[cfg(unix)]
fn fsync_dir(path: &Path) {
    if let Ok(dir) = std::fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

#[cfg(not(unix))]
fn fsync_dir(_path: &Path) {}

/// Build the seeded persistence `State` with one repository and one agent.
fn build_seeded_state(seed: &TierBStateSeed) -> State {
    let repo = Repository {
        id: RepositoryId(SEEDED_REPO_ID.to_string()),
        name: "Fixture Clone".to_string(),
        slug: "fixture-clone".to_string(),
        base_dir: seed.fixture_clone_path.clone(),
        default_profile: "llxprt".to_string(),
        default_code_puppy_model: String::new(),
        github_repo: seed.fixture_github_repo.clone(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: seed.agent_kind,
        agent_ids: vec![AgentId(SEEDED_AGENT_ID.to_string())],
    };
    let agent = Agent {
        id: AgentId(SEEDED_AGENT_ID.to_string()),
        display_id: seed.agent_name.clone(),
        repository_id: RepositoryId(SEEDED_REPO_ID.to_string()),
        shortcut_slot: None,
        name: seed.agent_name.clone(),
        description: "Tutorial capture seeded agent".to_string(),
        work_dir: seed.fixture_clone_path.clone(),
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
        agent_kind: seed.agent_kind,
        status: AgentStatus::Queued,
        runtime_binding: None,
    };
    State {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Finding #5: Return the TempDir RAII guard so it is automatically cleaned
    /// up when dropped, instead of calling .keep() which leaks the directory.
    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().value_or_panic("create temp dir")
    }

    fn sample_seed(config_dir: &Path) -> TierBStateSeed {
        TierBStateSeed {
            config_dir: config_dir.into(),
            fixture_clone_path: config_dir.join("fixture-clone"),
            fixture_github_repo: "fixture/test-repo".to_string(),
            theme: GREEN_SCREEN_THEME.to_string(),
            agent_name: "TutorialAgent".to_string(),
            agent_kind: AgentKind::Llxprt,
        }
    }

    #[test]
    fn seed_writes_settings_toml_with_theme() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let settings_path = config_dir.join("settings.toml");
        let content = std::fs::read_to_string(&settings_path).value_or_panic("read settings.toml");
        assert!(
            content.contains(GREEN_SCREEN_THEME),
            "settings must contain green-screen theme: {content}"
        );
    }

    /// Finding: settings.toml is written atomically — no leftover temp files
    /// remain in the config directory after seeding.
    #[test]
    fn seed_settings_toml_atomic_no_leftover_temps() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        // Verify no leftover temp files exist.
        let entries: Vec<_> = std::fs::read_dir(&config_dir)
            .value_or_panic("read config dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let temps: Vec<_> = entries
            .iter()
            .filter(|n| {
                n.starts_with('.')
                    && std::path::Path::new(n)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
            })
            .collect();
        assert!(
            temps.is_empty(),
            "no leftover temp files should exist after atomic write: {temps:?}"
        );
    }

    #[test]
    fn seed_writes_state_json_with_repository_and_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        assert_eq!(state.repositories.len(), 1, "must have one repository");
        assert_eq!(state.agents.len(), 1, "must have one agent");
    }

    #[test]
    fn seeded_repository_points_at_fixture_clone() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        let repo = &state.repositories[0];
        assert_eq!(
            repo.base_dir,
            config_dir.join("fixture-clone"),
            "repo base_dir must be fixture-clone path"
        );
    }

    #[test]
    fn seeded_repository_has_github_repo_association() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        let repo = &state.repositories[0];
        assert_eq!(
            repo.github_repo, "fixture/test-repo",
            "repo must have github_repo association"
        );
    }

    #[test]
    fn seeded_agent_is_bound_to_repository() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        let agent = &state.agents[0];
        assert_eq!(agent.name, "TutorialAgent");
        assert_eq!(
            agent.repository_id,
            RepositoryId(SEEDED_REPO_ID.to_string()),
            "agent must be bound to the seeded repository"
        );
    }

    #[test]
    fn seeded_state_has_correct_schema_version() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        assert_eq!(
            state.schema_version, STATE_SCHEMA_VERSION,
            "state must have correct schema version"
        );
    }

    #[test]
    fn seed_returns_repository_and_agent_ids() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        let result = seed_tier_b_state(&seed).value_or_panic("seed should succeed");
        assert_eq!(
            result.repository_id,
            RepositoryId(SEEDED_REPO_ID.to_string())
        );
        assert_eq!(result.agent_id, AgentId(SEEDED_AGENT_ID.to_string()));
    }

    // ── derive_agent_kind (Finding #4) ───────────────────────────────────

    #[test]
    fn derive_agent_kind_real_llxprt() {
        assert_eq!(
            derive_agent_kind(RuntimeProfile::RealLlxprt, ShimAvailability::Both),
            AgentKind::Llxprt
        );
    }

    #[test]
    fn derive_agent_kind_real_code_puppy() {
        assert_eq!(
            derive_agent_kind(RuntimeProfile::RealCodePuppy, ShimAvailability::Both),
            AgentKind::CodePuppy
        );
    }

    #[test]
    fn derive_agent_kind_shim_llxprt_only() {
        assert_eq!(
            derive_agent_kind(RuntimeProfile::Shim, ShimAvailability::LlxprtOnly),
            AgentKind::Llxprt
        );
    }

    #[test]
    fn derive_agent_kind_shim_code_puppy_only() {
        assert_eq!(
            derive_agent_kind(RuntimeProfile::Shim, ShimAvailability::CodePuppyOnly),
            AgentKind::CodePuppy
        );
    }

    #[test]
    fn derive_agent_kind_shim_both_defaults_to_llxprt() {
        assert_eq!(
            derive_agent_kind(RuntimeProfile::Shim, ShimAvailability::Both),
            AgentKind::Llxprt
        );
    }

    /// Finding #4: seeded state uses the agent_kind from the seed, not a
    /// hardcoded value. Verify a Code Puppy seed produces a Code Puppy agent.
    #[test]
    fn seed_with_code_puppy_kind_produces_code_puppy_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let fixture_clone = config_dir.join("fixture-clone");
        let seed = TierBStateSeed {
            config_dir: config_dir.clone(),
            fixture_clone_path: fixture_clone,
            fixture_github_repo: "fixture/test-repo".to_string(),
            theme: GREEN_SCREEN_THEME.to_string(),
            agent_name: "CodePuppyAgent".to_string(),
            agent_kind: AgentKind::CodePuppy,
        };
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        let agent = &state.agents[0];
        assert_eq!(
            agent.agent_kind,
            AgentKind::CodePuppy,
            "seeded agent must have CodePuppy kind"
        );
        let repo = &state.repositories[0];
        assert_eq!(
            repo.default_agent_kind,
            AgentKind::CodePuppy,
            "seeded repo must have CodePuppy default kind"
        );
    }

    /// Finding #4: seeded state with Llxprt kind produces a Llxprt agent.
    #[test]
    fn seed_with_llxprt_kind_produces_llxprt_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let fixture_clone = config_dir.join("fixture-clone");
        let seed = TierBStateSeed {
            config_dir: config_dir.clone(),
            fixture_clone_path: fixture_clone,
            fixture_github_repo: "fixture/test-repo".to_string(),
            theme: GREEN_SCREEN_THEME.to_string(),
            agent_name: "LlxprtAgent".to_string(),
            agent_kind: AgentKind::Llxprt,
        };
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        let agent = &state.agents[0];
        assert_eq!(agent.agent_kind, AgentKind::Llxprt);
    }

    /// Finding #5: Even when an empty theme is passed, settings.toml must
    /// contain the green-screen theme, not an empty value or a fallback to "dark".
    #[test]
    fn seed_empty_theme_falls_back_to_green_screen() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let fixture_clone = config_dir.join("fixture-clone");
        let seed = TierBStateSeed {
            config_dir: config_dir.clone(),
            fixture_clone_path: fixture_clone,
            fixture_github_repo: "fixture/test-repo".to_string(),
            theme: String::new(),
            agent_name: "TutorialAgent".to_string(),
            agent_kind: AgentKind::Llxprt,
        };
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let settings_path = config_dir.join("settings.toml");
        let content = std::fs::read_to_string(&settings_path).value_or_panic("read settings.toml");
        assert!(
            content.contains("green-screen"),
            "empty theme must default to green-screen: {content}"
        );
        assert!(
            !content.contains("dark"),
            "empty theme must NOT default to dark: {content}"
        );
    }

    /// Finding #5: The green-screen theme is always persisted when the
    /// standard green-screen theme string is passed.
    #[test]
    fn seed_green_screen_theme_is_persisted() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let settings_path = config_dir.join("settings.toml");
        let content = std::fs::read_to_string(&settings_path).value_or_panic("read settings.toml");
        assert!(
            content.contains("theme = \"green-screen\""),
            "settings.toml must persist green-screen theme explicitly: {content}"
        );
    }
}
