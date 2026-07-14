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

trait TestResultErrorExt<E> {
    fn error_or_panic(self, context: &str) -> E;
}

impl<E: std::fmt::Debug> TestResultErrorExt<E> for Result<(), E> {
    fn error_or_panic(self, context: &str) -> E {
        match self {
            Ok(()) => panic!("{context}: expected error"),
            Err(error) => error,
        }
    }
}

fn cleanup_root(path: &std::path::Path) -> Option<&std::path::Path> {
    path.parent().and_then(std::path::Path::parent)
}

/// Collect all entries under `dir` whose names contain `jefe-probe`.
fn leftover_probe_files(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.contains("jefe-probe").then_some(name)
        })
        .collect()
}

/// RAII guard that restores writable permissions (0o755) on a directory
/// when dropped, so cleanup always succeeds even if an assertion panics
/// while the directory is read-only.
#[cfg(unix)]
struct ReadOnlyDirGuard<'a> {
    dir: &'a std::path::Path,
    active: bool,
}

#[cfg(unix)]
impl Drop for ReadOnlyDirGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            use std::os::unix::fs::PermissionsExt;
            let writable = std::fs::Permissions::from_mode(0o755);
            let _ = std::fs::set_permissions(self.dir, writable);
        }
    }
}
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
fn resolve_paths_from_dir_roots_both_files_under_dir() {
    let dir = std::path::Path::new("/tmp/jefe-dev-instance");
    let paths = resolve_paths_from_dir(dir);
    assert_eq!(paths.settings_path, dir.join("settings.toml"));
    assert_eq!(paths.state_path, dir.join("state.json"));
}

#[test]
fn stub_persistence_returns_defaults() {
    let mgr = StubPersistenceManager::new();
    let settings = mgr.load_settings().value_or_panic("should load settings");
    assert_eq!(settings.theme, "green-screen");
}

#[test]
fn file_persistence_returns_defaults_when_missing() {
    let temp = unique_temp_root("missing_defaults");
    let _ = std::fs::remove_dir_all(&temp);
    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    let settings = mgr.load_settings().value_or_panic("should load defaults");
    assert_eq!(settings.theme, "green-screen");

    let state = mgr.load_state().value_or_panic("should load defaults");
    assert!(state.repositories.is_empty());

    let _ = std::fs::remove_dir_all(&temp);
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
        override_agent_theme: false,
    };

    mgr.save_settings(&settings).value_or_panic("should save");
    let loaded = mgr.load_settings().value_or_panic("should load");

    assert_eq!(loaded.theme, "dracula");
    assert_eq!(loaded.schema_version, SETTINGS_SCHEMA_VERSION);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp);
}

// ── Issue #179: override_agent_theme persistence ──────────────────────────

#[test]
fn override_agent_theme_defaults_false_in_default_with_version() {
    let settings = Settings::default_with_version();
    assert!(
        !settings.override_agent_theme,
        "override_agent_theme must default to false"
    );
}

#[test]
fn override_agent_theme_absent_in_toml_deserializes_false() {
    // A settings.toml without the override_agent_theme field (legacy file)
    // must deserialize with override_agent_theme == false.
    let toml_str = r#"
schema_version = 1
theme = "green-screen"
"#;
    let settings: Settings =
        toml::from_str(toml_str).value_or_panic("legacy settings should deserialize");
    assert!(
        !settings.override_agent_theme,
        "absent field must default to false"
    );
}

#[test]
fn override_agent_theme_true_round_trips() {
    let temp = std::env::temp_dir().join("jefe_test_override_theme_roundtrip");
    let _ = std::fs::remove_dir_all(&temp);
    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    let settings = Settings {
        schema_version: SETTINGS_SCHEMA_VERSION,
        theme: "dracula".into(),
        override_agent_theme: true,
    };

    mgr.save_settings(&settings).value_or_panic("should save");
    let loaded = mgr.load_settings().value_or_panic("should load");

    assert!(
        loaded.override_agent_theme,
        "override_agent_theme must survive a save/load round-trip when true"
    );

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
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: crate::domain::UserPreferences::default(),
    };
    mgr.save_state(&state).value_or_panic("should save");
    let loaded = mgr.load_state().value_or_panic("should load");

    assert_eq!(loaded.selected_repository_index, Some(2));
    assert!(loaded.hide_idle_repositories);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp);
}

/// Pane focus and terminal focus must survive a save/load round-trip (issue #160).
#[test]
fn file_persistence_roundtrip_pane_focus_and_terminal_focused() {
    let temp = std::env::temp_dir().join("jefe_test_roundtrip_focus");
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
        selected_repository_index: None,
        selected_agent_index: None,
        hide_idle_repositories: false,
        last_selected_agent_by_repo: vec![],
        pane_focus: "terminal".to_string(),
        terminal_focused: true,
        user_preferences: crate::domain::UserPreferences::default(),
    };
    mgr.save_state(&state).value_or_panic("should save");
    let loaded = mgr.load_state().value_or_panic("should load");

    assert_eq!(loaded.pane_focus, "terminal");
    assert!(loaded.terminal_focused);

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn file_persistence_atomic_write_creates_parent_dirs() {
    let temp = std::env::temp_dir()
        .join("jefe_test_atomic")
        .join("nested")
        .join("dirs");
    if let Some(root) = cleanup_root(&temp) {
        let _ = std::fs::remove_dir_all(root);
    }

    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    mgr.save_settings(&Settings::default_with_version())
        .value_or_panic("should create dirs and save");

    // Cleanup
    if let Some(root) = cleanup_root(&temp) {
        let _ = std::fs::remove_dir_all(root);
    }
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
        default_code_puppy_model: String::new(),
        github_repo: "fork-owner/issues-repo".to_string(),
        github_issue_pr_repo: "upstream-owner/issues-repo".to_string(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: "Always reproduce the bug first".to_string(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        transient_agent_dir: PathBuf::new(),
        default_code_puppy_yolo: None,
        transient_max_concurrent: 0,
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
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: crate::domain::UserPreferences::default(),
    };
    let temp = std::env::temp_dir().join("jefe_test_p13_issue_base_prompt_roundtrip");
    let _ = std::fs::remove_dir_all(&temp);
    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    mgr.save_state(&state).value_or_panic("should save state");
    let loaded = mgr.load_state().value_or_panic("should load state");

    assert_eq!(loaded.repositories.len(), 1);
    assert_eq!(
        loaded.repositories[0].issue_base_prompt,
        "Always reproduce the bug first"
    );
    assert_eq!(
        loaded.repositories[0].github_issue_pr_repo, "upstream-owner/issues-repo",
        "Issues / PRs repository override must survive persistence round-trip"
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
        serde_json::from_value(legacy_json).value_or_panic("legacy JSON should deserialize");

    assert_eq!(state.repositories.len(), 1);
    // Must default to empty string, not error
    assert_eq!(state.repositories[0].issue_base_prompt, "");
}

#[test]
fn validate_config_dir_rejects_regular_file() {
    // A path that exists as a regular file (not a directory) must be
    // rejected with a clear, context-rich error mentioning the config path.
    let temp = std::env::temp_dir().join("jefe_test_validate_regular_file");
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_dir_all(&temp);

    std::fs::write(&temp, "not a directory").value_or_panic("should seed regular file");

    let error = validate_config_dir(&temp).error_or_panic("should reject regular file");
    let PersistenceError::InvalidConfigDir { path, reason } = &error else {
        panic!("expected InvalidConfigDir, got {error:?}");
    };
    assert_eq!(path, &temp, "error must mention the config path");
    assert!(
        reason.contains("not a directory"),
        "reason should explain it is not a directory, got: {reason}"
    );

    let _ = std::fs::remove_file(&temp);
}

#[test]
fn validate_config_dir_succeeds_for_fresh_temp_dir_and_leaves_no_probe() {
    // A freshly-created writable temp directory should validate
    // successfully and must not leave any probe file behind.
    let temp = std::env::temp_dir().join("jefe_test_validate_fresh_dir");
    let _ = std::fs::remove_dir_all(&temp);

    validate_config_dir(&temp).value_or_panic("fresh writable dir should validate");

    assert!(temp.is_dir(), "directory should exist after validation");

    // No probe file of any kind should remain after a successful validation.
    let leftover_probes = leftover_probe_files(&temp);
    assert!(
        leftover_probes.is_empty(),
        "no probe files should remain, found: {leftover_probes:?}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn validate_config_dir_creates_missing_directory() {
    // A non-existent nested directory should be created and validate.
    let temp = std::env::temp_dir().join("jefe_test_validate_create_missing");
    let nested = temp.join("a").join("b");
    let _ = std::fs::remove_dir_all(&temp);

    validate_config_dir(&nested).value_or_panic("should create nested dir and validate");
    assert!(nested.is_dir(), "nested directory should be created");

    let _ = std::fs::remove_dir_all(&temp);
}

/// Behavioral test for the core issue #65 scenario: an explicit config
/// directory that exists but cannot be written to must be rejected
/// fail-fast with a clear, actionable error.
///
/// This is Unix-gated because it relies on POSIX permission semantics
/// (chmod). When tests run as effective root, chmod restrictions are
/// bypassed (root can write anywhere), so the test detects this by probing
/// whether the read-only directory actually rejects a write and skips if
/// not.
#[cfg(unix)]
#[test]
fn validate_config_dir_rejects_unwritable_directory() {
    use std::os::unix::fs::PermissionsExt;

    let temp = std::env::temp_dir().join("jefe_test_validate_unwritable");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).value_or_panic("should create test dir");

    // Set the directory to read-only (no write for anyone).
    let read_only = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&temp, read_only).value_or_panic("should set read-only permissions");

    // Guard restores writability on drop so cleanup always succeeds, even
    // if an assertion panics.
    let guard = ReadOnlyDirGuard {
        dir: &temp,
        active: true,
    };

    // Detect environments where chmod does not enforce writability (e.g.
    // effective root). If a write succeeds despite 0o555, skip rather than
    // produce a false negative.
    let probe = temp.join("writability-check");
    let chmod_enforced = std::fs::File::create(&probe).is_err();
    let _ = std::fs::remove_file(&probe);
    if !chmod_enforced {
        // chmod did not prevent writes (likely running as root); skip.
        return;
    }

    let error = validate_config_dir(&temp).error_or_panic("unwritable dir should fail validation");
    let PersistenceError::InvalidConfigDir { path, reason } = &error else {
        panic!("expected InvalidConfigDir, got {error:?}");
    };
    assert_eq!(path, &temp, "error must mention the config path");
    assert!(
        reason.contains("settings.toml")
            || reason.contains("state.json")
            || reason.contains("create"),
        "reason should reference the persistence file or creation, got: {reason}"
    );

    drop(guard);
    let _ = std::fs::remove_dir_all(&temp);
}

/// Regression test for the probe-write data-loss hazard: `probe_write`
/// must never truncate or remove a pre-existing user file. Even if a file
/// matching a probe-like name exists, validation must fail (via
/// `create_new`) without touching the pre-existing file's contents.
#[test]
fn validate_config_dir_does_not_truncate_or_remove_existing_files() {
    let temp = std::env::temp_dir().join("jefe_test_validate_no_truncate");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).value_or_panic("should create test dir");

    // The real persistence files (settings.toml/state.json) must be left
    // untouched by validation. Seed a settings.toml with real content.
    let settings_path = temp.join("settings.toml");
    let original_content = "important user data";
    std::fs::write(&settings_path, original_content).value_or_panic("should seed settings.toml");

    // Validation should succeed (the dir is writable) and must not alter
    // the pre-existing settings.toml.
    validate_config_dir(&temp).value_or_panic("writable dir with existing files should validate");

    let after = std::fs::read_to_string(&settings_path)
        .value_or_panic("settings.toml should still be readable");
    assert_eq!(
        after, original_content,
        "probe_write must not truncate or alter a pre-existing settings.toml"
    );

    // No probe file should remain.
    let leftover_probes = leftover_probe_files(&temp);
    assert!(
        leftover_probes.is_empty(),
        "no probe files should remain, found: {leftover_probes:?}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

/// Helper that creates a unique temp root for a test under the system temp
/// directory, tagged with a label and the current process id so concurrent
/// test runs never collide. The returned path does not yet exist.
fn unique_temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "jefe_test_validate_target_{label}_{}_{}",
        std::process::id(),
        probe_counter()
    ))
}

/// Regression: validation must reject the case where the actual
/// `settings.toml` target already exists as a directory under an otherwise
/// writable explicit config dir. Without this check, `atomic_write`
/// (`File::create` on a `.tmp` sibling then `rename` onto the directory)
/// would fail at runtime while `build_persistence` would have already
/// succeeded, leaving persistence silently broken.
#[test]
fn validate_config_dir_rejects_settings_toml_as_directory() {
    let temp = unique_temp_root("settings_is_dir");
    std::fs::create_dir_all(&temp).value_or_panic("should create config dir");

    // Create settings.toml as a directory, blocking the real atomic_write
    // target.
    let settings_dir = temp.join("settings.toml");
    std::fs::create_dir_all(&settings_dir)
        .value_or_panic("should seed settings.toml as a directory");

    let error = validate_config_dir(&temp)
        .error_or_panic("should reject when settings.toml target is a directory");
    let PersistenceError::InvalidConfigDir { path, reason } = &error else {
        panic!("expected InvalidConfigDir, got {error:?}");
    };
    assert_eq!(path, &temp, "error must mention the config path");
    assert!(
        reason.contains("settings.toml"),
        "reason should name the settings.toml target, got: {reason}"
    );
    assert!(
        reason.contains("directory") || reason.contains("not a regular file"),
        "reason should explain the target is not a regular file, got: {reason}"
    );

    // The blocking directory the test created must not be removed by
    // validation (it is user data, not a probe).
    assert!(
        settings_dir.is_dir(),
        "validation must not remove the user-created settings.toml directory"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

/// Regression: validation must reject the case where the actual
/// `state.json` target already exists as a directory under an otherwise
/// writable explicit config dir. Same rationale as the settings.toml case.
#[test]
fn validate_config_dir_rejects_state_json_as_directory() {
    let temp = unique_temp_root("state_is_dir");
    std::fs::create_dir_all(&temp).value_or_panic("should create config dir");

    let state_dir = temp.join("state.json");
    std::fs::create_dir_all(&state_dir).value_or_panic("should seed state.json as a directory");

    let error = validate_config_dir(&temp)
        .error_or_panic("should reject when state.json target is a directory");
    let PersistenceError::InvalidConfigDir { path, reason } = &error else {
        panic!("expected InvalidConfigDir, got {error:?}");
    };
    assert_eq!(path, &temp, "error must mention the config path");
    assert!(
        reason.contains("state.json"),
        "reason should name the state.json target, got: {reason}"
    );
    assert!(
        reason.contains("directory") || reason.contains("not a regular file"),
        "reason should explain the target is not a regular file, got: {reason}"
    );

    assert!(
        state_dir.is_dir(),
        "validation must not remove the user-created state.json directory"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

/// Regression: validation must reject atomic-write temporary siblings that
/// already exist as directories. Otherwise startup would pass, but each save
/// would fail while creating the deterministic temp file.
#[test]
fn validate_config_dir_rejects_atomic_tmp_targets_as_directories() {
    for tmp_name in ["settings.tmp", "state.tmp"] {
        let temp = unique_temp_root(tmp_name);
        std::fs::create_dir_all(&temp).value_or_panic("should create config dir");

        let tmp_dir = temp.join(tmp_name);
        std::fs::create_dir_all(&tmp_dir).value_or_panic("should seed tmp target as a directory");

        let error = validate_config_dir(&temp)
            .error_or_panic("should reject when tmp target is a directory");
        let PersistenceError::InvalidConfigDir { path, reason } = &error else {
            panic!("expected InvalidConfigDir, got {error:?}");
        };
        assert_eq!(path, &temp, "error must mention the config path");
        assert!(
            reason.contains(tmp_name),
            "reason should name the {tmp_name} target, got: {reason}"
        );
        assert!(
            tmp_dir.is_dir(),
            "validation must not remove the user-created {tmp_name} directory"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }
}

#[cfg(unix)]
#[test]
fn validate_config_dir_rejects_unwritable_atomic_tmp_files() {
    use std::os::unix::fs::PermissionsExt;

    for tmp_name in ["settings.tmp", "state.tmp"] {
        let temp = unique_temp_root(tmp_name);
        std::fs::create_dir_all(&temp).value_or_panic("should create config dir");

        let tmp_path = temp.join(tmp_name);
        std::fs::write(&tmp_path, "existing temp file").value_or_panic("should seed tmp file");
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o444))
            .value_or_panic("should set tmp file read-only");

        let chmod_enforced = std::fs::OpenOptions::new()
            .write(true)
            .open(&tmp_path)
            .is_err();
        if !chmod_enforced {
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o644));
            let _ = std::fs::remove_dir_all(&temp);
            continue;
        }

        let error = validate_config_dir(&temp)
            .error_or_panic("should reject when tmp target cannot be opened for writing");
        let PersistenceError::InvalidConfigDir { path, reason } = &error else {
            panic!("expected InvalidConfigDir, got {error:?}");
        };
        assert_eq!(path, &temp, "error must mention the config path");
        assert!(
            reason.contains(tmp_name),
            "reason should name the {tmp_name} target, got: {reason}"
        );
        assert!(
            reason.contains("atomic writes") || reason.contains("Permission denied"),
            "reason should explain the temp target is not writable, got: {reason}"
        );

        let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o644));
        let _ = std::fs::remove_dir_all(&temp);
    }
}

#[test]
fn default_themes_dir_ends_with_themes_subdir() {
    // Use the pure helper to avoid reading real env vars.
    let config_dir = resolve_config_dir_from_env(None, None);
    let themes_dir = config_dir.join("themes");
    // Verify the themes dir is a child of the config dir and ends with "themes".
    assert!(themes_dir.starts_with(&config_dir));
    assert!(themes_dir.ends_with("themes"));
}

#[test]
fn resolve_config_dir_prefers_jefe_config_dir() {
    // With only JEFE_CONFIG_DIR set (no settings path), it's used directly.
    let dir = resolve_config_dir_from_env(None, Some("/custom/config".into()));
    assert_eq!(dir, PathBuf::from("/custom/config"));
}

#[test]
fn resolve_config_dir_settings_path_takes_precedence_over_config_dir() {
    // When both are set, JEFE_SETTINGS_PATH's parent wins (mirrors resolve_settings_path).
    let dir = resolve_config_dir_from_env(
        Some("/other/settings.toml".into()),
        Some("/custom/config".into()),
    );
    assert_eq!(dir, PathBuf::from("/other"));
}

#[test]
fn resolve_config_dir_uses_settings_path_parent_when_no_config_dir() {
    let dir = resolve_config_dir_from_env(Some("/a/b/settings.toml".into()), None);
    assert_eq!(dir, PathBuf::from("/a/b"));
}

#[test]
fn resolve_config_dir_ignores_bare_filename_settings_path() {
    let dir = resolve_config_dir_from_env(Some("settings.toml".into()), None);
    assert!(dir.ends_with("jefe"));
}

#[test]
fn resolve_config_dir_falls_back_to_platform_default() {
    let dir = resolve_config_dir_from_env(None, None);
    assert!(dir.ends_with("jefe"));
}

#[test]
fn resolve_config_dir_ignores_empty_env_values() {
    let dir = resolve_config_dir_from_env(Some(String::new()), Some(String::new()));
    assert!(dir.ends_with("jefe"));
}

// ── Issue #163: user_preferences round-trip + backward compat ─────────────

#[test]
fn user_preferences_roundtrip() {
    use crate::domain::{
        ChecksFilter, IssueFilter, IssueFilterState, MergeMethod, PrFilter, PrFilterState,
        RepoPreferences, RepositoryId, ReviewDecisionFilter, UserPreferences,
    };

    let prefs = UserPreferences {
        by_repo: vec![(
            RepositoryId("repo-1".to_string()),
            RepoPreferences {
                issue_filter: IssueFilter {
                    state: Some(IssueFilterState::Closed),
                    author: "alice".to_string(),
                    assignee: "bob".to_string(),
                    labels: vec!["bug".to_string(), "ui".to_string()],
                    milestone: "v1".to_string(),
                    ..IssueFilter::default()
                },
                pr_filter: PrFilter {
                    state: Some(PrFilterState::Merged),
                    author: "carol".to_string(),
                    reviewer: "dave".to_string(),
                    is_draft: Some(true),
                    review_decision: ReviewDecisionFilter::Approved,
                    checks_status: ChecksFilter::Success,
                    labels: vec!["needs-review".to_string()],
                    ..PrFilter::default()
                },
                issue_search_query: "issue-search".to_string(),
                pr_search_query: "pr-search".to_string(),
                issue_filter_field_index: 3,
                pr_filter_field_index: 5,
                last_merge_method: Some(MergeMethod::Squash),
            },
        )],
    };

    let state = State {
        user_preferences: prefs,
        ..State::default_with_version()
    };

    let json = serde_json::to_string(&state).value_or_panic("serialize state");
    let restored: State = serde_json::from_str(&json).value_or_panic("deserialize state");
    assert_eq!(restored.user_preferences, state.user_preferences);
}

#[test]
fn legacy_state_without_preferences_deserializes_to_default() {
    // A legacy state.json that predates the user_preferences field must
    // deserialize cleanly, yielding default (empty) preferences.
    let legacy_json = r#"{
        "schema_version": 1,
        "repositories": [],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    }"#;
    let state: State = serde_json::from_str(legacy_json).value_or_panic("deserialize legacy state");
    assert!(state.user_preferences.by_repo.is_empty());
}

// ── Issue #163 FIX 8: Full restart-hydration integration test ─────────────

/// End-to-end restart test: save → load → restore → enter-mode proves no
/// cross-repo leakage through the real persistence layer.
#[test]
fn restart_hydration_preserves_per_repo_preferences() {
    use crate::domain::{
        IssueFilter, IssueFilterState, PrFilter, PrFilterState, RepoPreferences, Repository,
        RepositoryId, UserPreferences,
    };

    let repo1 = Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/repo2"),
    );

    let prefs1 = RepoPreferences {
        issue_filter: IssueFilter {
            state: Some(IssueFilterState::Closed),
            ..IssueFilter::default()
        },
        pr_filter: PrFilter {
            state: Some(PrFilterState::Merged),
            ..PrFilter::default()
        },
        pr_search_query: "alpha".to_string(),
        ..RepoPreferences::default()
    };
    let prefs2 = RepoPreferences {
        issue_filter: IssueFilter {
            state: Some(IssueFilterState::All),
            ..IssueFilter::default()
        },
        ..RepoPreferences::default()
    };

    let persisted = State {
        schema_version: STATE_SCHEMA_VERSION,
        repositories: vec![repo1, repo2],
        user_preferences: UserPreferences {
            by_repo: vec![
                (RepositoryId("repo-1".to_string()), prefs1),
                (RepositoryId("repo-2".to_string()), prefs2),
            ],
        },
        ..State::default_with_version()
    };

    let loaded = save_load_roundtrip(&persisted, "jefe_test_restart_hydration_prefs");

    // Round-trip: both repos' prefs intact with correct states + search query.
    let r1 = loaded
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(r1.issue_filter.state, Some(IssueFilterState::Closed));
    assert_eq!(r1.pr_filter.state, Some(PrFilterState::Merged));
    assert_eq!(r1.pr_search_query, "alpha");

    let r2 = loaded
        .user_preferences
        .for_repo(&RepositoryId("repo-2".to_string()));
    assert_eq!(r2.issue_filter.state, Some(IssueFilterState::All));

    // Simulate init_app_state's restore: copy loaded prefs onto fresh AppState.
    verify_mode_entry_restore(&loaded);
}

/// Save and load a State through the real FilePersistenceManager into a temp
/// dir tagged with `label` and the current process id (so parallel test
/// invocations never collide and a crash never leaves stale data for the next
/// run). Returns the deserialized State.
fn save_load_roundtrip(persisted: &State, label: &str) -> State {
    let temp = std::env::temp_dir().join(format!("{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    mgr.save_state(persisted).value_or_panic("should save");
    let loaded = mgr.load_state().value_or_panic("should load");

    let _ = std::fs::remove_dir_all(&temp);
    loaded
}

/// Given a loaded persistence::State, simulate init_app_state's restore onto a
/// fresh AppState, then verify entering issues mode for each repo restores the
/// correct per-repo filter state without cross-repo leakage.
fn verify_mode_entry_restore(loaded: &State) {
    use crate::domain::IssueFilterState;
    use crate::state::{AppEvent, AppState};

    let state = AppState {
        repositories: loaded.repositories.clone(),
        user_preferences: loaded.user_preferences.clone(),
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    // repo-1 → Closed.
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Closed)
    );

    // repo-2 → All (no leakage from repo-1).
    let state = state.apply(AppEvent::ExitIssuesMode);
    let state = state.apply(AppEvent::SelectRepository(1));
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::All)
    );
}

/// A legacy state.json (without user_preferences) deserializes and then
/// entering a mode gives Open defaults.
#[test]
fn restart_hydration_legacy_state_gives_open_defaults_on_mode_entry() {
    use crate::domain::{IssueFilterState, RepositoryId};
    use crate::state::{AppEvent, AppState};

    let legacy_json = r#"{
        "schema_version": 1,
        "repositories": [
            {
                "id": "legacy-repo",
                "name": "Legacy",
                "slug": "legacy",
                "base_dir": "/tmp/legacy",
                "default_profile": "",
                "agent_ids": []
            }
        ],
        "agents": [],
        "selected_repository_index": 0,
        "selected_agent_index": null
    }"#;
    let loaded: State =
        serde_json::from_str(legacy_json).value_or_panic("deserialize legacy state");

    let state = AppState {
        repositories: loaded.repositories.clone(),
        user_preferences: loaded.user_preferences.clone(),
        selected_repository_index: Some(0),
        ..AppState::default()
    };

    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Open)
    );

    // Ensure the repo is actually the one we loaded.
    assert_eq!(
        state.repositories[0].id,
        RepositoryId("legacy-repo".to_string())
    );
}
