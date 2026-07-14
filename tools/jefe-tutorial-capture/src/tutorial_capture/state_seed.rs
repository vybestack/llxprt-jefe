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
//! ## Architecture boundary
//!
//! This module owns all tutorial-shaped policy: the one fixture
//! [`Repository`], one [`Agent`], stable IDs, queued/default fields, the
//! settings theme, and selection indices. It constructs fully-populated
//! canonical [`Settings`] and [`State`] values and delegates only the
//! transactional file I/O to the root-owned
//! [`jefe::persistence::seed::seed_isolated_config`] API. The root
//! persistence layer acts solely as a serializer/I/O owner — it does not
//! construct repository/agent/tutorial policy.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::path::{Path, PathBuf};

use jefe::domain::{
    Agent, AgentId, AgentKind, AgentStatus, Repository, RepositoryId, SandboxEngine,
};
use jefe::persistence::seed::{SeedError, SeedRequest, seed_isolated_config};
use jefe::persistence::{SETTINGS_SCHEMA_VERSION, STATE_SCHEMA_VERSION, Settings, State};

use super::manifest::RuntimeProfile;
use super::path_shim::ShimAvailability;

/// Error returned by state seeding operations.
#[derive(Debug)]
pub struct StateSeedError {
    reason: String,
}

impl std::fmt::Display for StateSeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "state seed error: {}", self.reason)
    }
}

impl std::error::Error for StateSeedError {}

impl From<SeedError> for StateSeedError {
    fn from(value: SeedError) -> Self {
        Self {
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

/// Derive the agent kind from the runtime profile and shim availability.
///
/// Instead of hardcoding `AgentKind::Llxprt`, this function derives the seed
/// agent kind from the manifest's runtime profile and shim availability:
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
/// Constructs the one fixture [`Repository`], one [`Agent`], stable IDs,
/// queued/default fields, settings theme, and selection indices — all
/// tutorial-shaped policy owned by the tool — then delegates the
/// transactional file I/O to the root persistence-owned
/// [`jefe::persistence::seed::seed_isolated_config`] API.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`StateSeedError`] if the persistence-layer seed operation fails.
pub fn seed_tier_b_state(seed: &TierBStateSeed) -> Result<SeededState, StateSeedError> {
    let repository_id = RepositoryId(SEEDED_REPO_ID.to_string());
    let agent_id = AgentId(SEEDED_AGENT_ID.to_string());

    let settings = build_settings(&seed.theme);
    let state = build_state(
        &repository_id,
        &agent_id,
        &seed.fixture_clone_path,
        &seed.fixture_github_repo,
        &seed.agent_name,
        seed.agent_kind,
    );

    let request = SeedRequest {
        config_dir: seed.config_dir.clone(),
        settings,
        state,
    };
    seed_isolated_config(&request)?;

    Ok(SeededState {
        repository_id,
        agent_id,
    })
}

/// Build canonical [`Settings`] for the tutorial-capture run.
///
/// The tool owns the theme; an empty theme falls back to `"green-screen"`.
fn build_settings(theme: &str) -> Settings {
    Settings {
        schema_version: SETTINGS_SCHEMA_VERSION,
        theme: if theme.is_empty() {
            String::from("green-screen")
        } else {
            theme.to_string()
        },
        override_agent_theme: false,
    }
}

/// Build canonical [`State`] containing the one fixture repository and one
/// agent. All tutorial shape/policy is owned by the tool.
fn build_state(
    repository_id: &RepositoryId,
    agent_id: &AgentId,
    work_dir: &Path,
    github_repo: &str,
    agent_name: &str,
    agent_kind: AgentKind,
) -> State {
    let profile = agent_kind.binary_name().to_string();
    let repo = Repository {
        id: repository_id.clone(),
        name: "Tutorial Fixture".to_string(),
        slug: SEEDED_REPO_ID.to_string(),
        base_dir: work_dir.to_path_buf(),
        default_profile: profile.clone(),
        default_code_puppy_model: String::new(),
        github_repo: github_repo.to_string(),
        remote: jefe::domain::RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: agent_kind,
        agent_ids: vec![agent_id.clone()],
    };
    let agent = Agent {
        id: agent_id.clone(),
        display_id: agent_name.to_string(),
        repository_id: repository_id.clone(),
        shortcut_slot: None,
        name: agent_name.to_string(),
        description: "Tutorial capture agent".to_string(),
        work_dir: work_dir.to_path_buf(),
        profile,
        code_puppy_model: String::new(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::default(),
        sandbox_flags: String::new(),
        agent_kind,
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
        user_preferences: jefe::domain::UserPreferences::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::persistence::State;

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
        tempfile::tempdir().value_or_panic("create temp dir")
    }

    fn sample_seed(config_dir: &std::path::Path) -> TierBStateSeed {
        TierBStateSeed {
            config_dir: config_dir.into(),
            fixture_clone_path: config_dir.join("fixture-clone"),
            fixture_github_repo: "fixture/test-repo".to_string(),
            theme: "green-screen".to_string(),
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
            content.contains("green-screen"),
            "settings must contain green-screen theme: {content}"
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

    #[test]
    fn seeded_repository_has_tutorial_fixture_name() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        assert_eq!(
            state.repositories[0].name, "Tutorial Fixture",
            "tool must own the repository display name"
        );
    }

    #[test]
    fn seeded_agent_has_tutorial_capture_description() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        assert_eq!(
            state.agents[0].description, "Tutorial capture agent",
            "tool must own the agent description"
        );
    }

    #[test]
    fn seeded_state_has_selection_indices_pointing_at_repo_and_agent() {
        let dir = temp_dir();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).value_or_panic("create config dir");
        let seed = sample_seed(&config_dir);
        seed_tier_b_state(&seed).value_or_panic("seed should succeed");

        let state_path = config_dir.join("state.json");
        let content = std::fs::read_to_string(&state_path).value_or_panic("read state.json");
        let state: State = serde_json::from_str(&content).value_or_panic("parse state.json");
        assert_eq!(state.selected_repository_index, Some(0));
        assert_eq!(state.selected_agent_index, Some(0));
    }

    // ── derive_agent_kind ────────────────────────────────────────────────

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
            theme: "green-screen".to_string(),
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
            theme: "green-screen".to_string(),
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
    }
}
