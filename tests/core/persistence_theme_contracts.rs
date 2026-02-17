//! Persistence and theme contract tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P04
//! @requirement REQ-FUNC-001
//! @requirement REQ-FUNC-009
//! @requirement REQ-TECH-005
//!
//! Pseudocode reference: component-003 lines 01-44

#![allow(clippy::expect_used)]

use jefe::persistence::{
    PersistenceManager, SETTINGS_SCHEMA_VERSION, STATE_SCHEMA_VERSION, Settings, State,
    StubPersistenceManager, resolve_paths,
};
use jefe::theme::{StubThemeManager, ThemeDefinition, ThemeKind, ThemeManager};

// =============================================================================
// Persistence Path Resolution (REQ-FUNC-001)
// Pseudocode: component-003 lines 01-08
//
// NOTE: Env var override tests require unsafe (Rust 2024) and are tested
// via integration tests with a separate binary that allows unsafe.
// Here we test the path structure invariants.
// =============================================================================

#[test]
fn resolved_settings_path_ends_with_settings_toml() {
    let paths = resolve_paths();
    assert!(
        paths.settings_path.ends_with("settings.toml"),
        "settings path must end with settings.toml"
    );
}

#[test]
fn resolved_state_path_ends_with_state_json() {
    let paths = resolve_paths();
    assert!(
        paths.state_path.ends_with("state.json"),
        "state path must end with state.json"
    );
}

#[test]
fn resolved_paths_are_absolute_or_relative_to_jefe() {
    let paths = resolve_paths();
    // Either absolute paths or .jefe fallback
    let settings_ok = paths.settings_path.is_absolute() || paths.settings_path.starts_with(".jefe");
    let state_ok = paths.state_path.is_absolute() || paths.state_path.starts_with(".jefe");
    assert!(
        settings_ok,
        "settings path must be absolute or .jefe relative"
    );
    assert!(state_ok, "state path must be absolute or .jefe relative");
}

// =============================================================================
// Persistence Defaults (REQ-FUNC-001)
// =============================================================================

#[test]
fn settings_default_has_green_screen_theme() {
    let settings = Settings::default_with_version();
    assert_eq!(
        settings.theme, "green-screen",
        "default theme must be green-screen per REQ-FUNC-009"
    );
}

#[test]
fn settings_default_has_current_schema_version() {
    let settings = Settings::default_with_version();
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
}

#[test]
fn state_default_has_current_schema_version() {
    let state = State::default_with_version();
    assert_eq!(state.schema_version, STATE_SCHEMA_VERSION);
}

#[test]
fn state_default_has_empty_collections() {
    let state = State::default_with_version();
    assert!(state.repositories.is_empty());
    assert!(state.agents.is_empty());
    assert!(state.selected_repository_index.is_none());
    assert!(state.selected_agent_index.is_none());
}

// =============================================================================
// Persistence Load/Save Behavior (REQ-FUNC-001)
// Pseudocode: component-003 lines 09-22
// =============================================================================

#[test]
fn load_settings_returns_defaults_when_file_missing() {
    let mgr = StubPersistenceManager::new();
    let settings = mgr.load_settings().expect("should return defaults");
    assert_eq!(settings.theme, "green-screen");
}

#[test]
fn load_state_returns_defaults_when_file_missing() {
    let mgr = StubPersistenceManager::new();
    let state = mgr.load_state().expect("should return defaults");
    assert!(state.repositories.is_empty());
}

// These tests will need real implementation in P05:
// - load_settings_parses_valid_toml
// - load_settings_returns_defaults_on_parse_error
// - load_state_parses_valid_json
// - load_state_returns_defaults_on_parse_error
// - save_settings_writes_atomic (temp + rename)
// - save_state_writes_atomic (temp + rename)
// - save_creates_parent_directories

// =============================================================================
// Theme Resolution (REQ-FUNC-009)
// Pseudocode: component-003 lines 23-44
// =============================================================================

#[test]
fn default_theme_is_green_screen() {
    let mgr = StubThemeManager::new();
    let theme = mgr.active_theme();
    assert_eq!(
        theme.slug, "green-screen",
        "default theme must be Green Screen"
    );
    assert_eq!(theme.kind, ThemeKind::Dark);
}

#[test]
fn green_screen_has_correct_colors() {
    let theme = ThemeDefinition::green_screen();
    assert_eq!(
        theme.colors.background, "#000000",
        "Green Screen background must be black"
    );
    assert_eq!(
        theme.colors.foreground, "#6a9955",
        "Green Screen foreground must be green"
    );
    assert_eq!(
        theme.colors.accent_success, "#00ff00",
        "Green Screen success must be bright green"
    );
}

#[test]
fn resolve_unknown_theme_returns_green_screen() {
    let mgr = StubThemeManager::new();
    let theme = mgr.resolve("nonexistent-theme");
    assert_eq!(
        theme.slug, "green-screen",
        "unknown theme must fallback to Green Screen"
    );
}

#[test]
fn set_active_unknown_falls_back_to_green_screen() {
    let mut mgr = StubThemeManager::new();
    let result = mgr.set_active("nonexistent");
    assert!(
        result.is_err(),
        "set_active should return error for unknown theme"
    );
    assert_eq!(
        mgr.active_theme().slug,
        "green-screen",
        "active theme should fallback to Green Screen"
    );
}

#[test]
fn available_themes_includes_green_screen() {
    let mgr = StubThemeManager::new();
    let themes = mgr.available_themes();
    assert!(
        themes.contains(&"green-screen".to_string()),
        "Green Screen must be in available themes"
    );
}

// =============================================================================
// Theme Kind Classification
// =============================================================================

#[test]
fn green_screen_is_dark_theme() {
    let theme = ThemeDefinition::green_screen();
    assert_eq!(theme.kind, ThemeKind::Dark);
}

// =============================================================================
// Integration: Settings Theme -> ThemeManager
// =============================================================================

#[test]
fn settings_theme_applies_to_theme_manager() {
    let settings = Settings {
        schema_version: SETTINGS_SCHEMA_VERSION,
        theme: "green-screen".into(),
    };

    let mut mgr = StubThemeManager::new();
    let result = mgr.set_active(&settings.theme);

    assert!(result.is_ok(), "valid theme from settings should apply");
    assert_eq!(mgr.active_theme().slug, "green-screen");
}

#[test]
fn invalid_settings_theme_falls_back() {
    let settings = Settings {
        schema_version: SETTINGS_SCHEMA_VERSION,
        theme: "bogus-theme".into(),
    };

    let mut mgr = StubThemeManager::new();
    let result = mgr.set_active(&settings.theme);

    assert!(result.is_err(), "invalid theme should return error");
    assert_eq!(
        mgr.active_theme().slug,
        "green-screen",
        "should fallback to Green Screen"
    );
}
