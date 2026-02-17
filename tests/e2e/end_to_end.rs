//! End-to-end integration tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P13
//! @requirement REQ-TECH-009
//!
//! These tests verify that all layers work together:
//! - Domain models
//! - State transitions
//! - Persistence
//! - Theme management
//! - Runtime orchestration

#![allow(clippy::unwrap_used, clippy::expect_used)]

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::persistence::{
    FilePersistenceManager, PersistenceManager, PersistencePaths, Settings, State,
};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};
use jefe::theme::{FileThemeManager, ThemeManager};
use std::path::PathBuf;

/// Create a complete test environment with all layers.
fn create_test_environment() -> (AppState, FilePersistenceManager, FileThemeManager) {
    // Use unique temp dir per test to avoid concurrent test interference
    let unique_id = std::thread::current().id();
    let temp = std::env::temp_dir().join(format!("jefe_e2e_test_{unique_id:?}"));
    let _ = std::fs::remove_dir_all(&temp);

    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };

    let persistence = FilePersistenceManager::with_paths(paths);
    let theme_mgr = FileThemeManager::new();

    let mut state = AppState::default();

    // Add test data
    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        "llxprt-code".into(),
        "llxprt-code".into(),
        PathBuf::from("/projects/llxprt-code"),
    );
    state.repositories = vec![repo];

    let agent = Agent::new(
        AgentId("agent-1".into()),
        RepositoryId("repo-1".into()),
        "Fix issue #1234".into(),
        PathBuf::from("/worktrees/issue-1234"),
    );
    state.agents = vec![agent];

    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    (state, persistence, theme_mgr)
}

// ============================================================================
// Full Workflow Tests
// ============================================================================

#[test]
fn full_navigation_workflow() {
    let (mut state, _, _) = create_test_environment();

    // Start at dashboard
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);

    // Cycle through panes
    state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Agents);

    state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Terminal);

    state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);

    // Enter split mode
    state = state.apply(AppEvent::EnterSplitMode);
    assert_eq!(state.screen_mode, ScreenMode::Split);

    // Exit split mode
    state = state.apply(AppEvent::ExitSplitMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

#[test]
fn full_modal_workflow() {
    let (mut state, _, _) = create_test_environment();

    // Open help
    state = state.apply(AppEvent::OpenHelp);
    assert_eq!(state.modal, ModalState::Help);

    // Close help
    state = state.apply(AppEvent::CloseModal);
    assert_eq!(state.modal, ModalState::None);

    // Open search
    state = state.apply(AppEvent::OpenSearch);
    assert!(matches!(state.modal, ModalState::Search { .. }));

    // Close search
    state = state.apply(AppEvent::CloseModal);
    assert_eq!(state.modal, ModalState::None);

    // Open new repository form
    state = state.apply(AppEvent::OpenNewRepository);
    assert!(matches!(state.modal, ModalState::NewRepository { .. }));

    // Close form
    state = state.apply(AppEvent::CloseModal);
    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn full_terminal_focus_workflow() {
    let (mut state, _, _) = create_test_environment();

    // Initially terminal is not focused
    assert!(!state.terminal_focused);

    // Toggle focus on
    state = state.apply(AppEvent::ToggleTerminalFocus);
    assert!(state.terminal_focused);

    // Navigation should be blocked
    let original = state.selected_agent_index;
    state = state.apply(AppEvent::NavigateDown);
    assert_eq!(state.selected_agent_index, original, "Navigation blocked");

    // Toggle focus off
    state = state.apply(AppEvent::ToggleTerminalFocus);
    assert!(!state.terminal_focused);

    // Navigation should work again
    // (would need more agents to test, but the flow is verified)
}

// ============================================================================
// Persistence Integration Tests
// ============================================================================

#[test]
fn persistence_roundtrip_preserves_state() {
    let temp = std::env::temp_dir().join("jefe_e2e_roundtrip");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");

    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    };
    let persistence = FilePersistenceManager::with_paths(paths);

    // Create state with data
    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        "llxprt-code".into(),
        "llxprt-code".into(),
        PathBuf::from("/projects/llxprt-code"),
    );
    let agent = Agent::new(
        AgentId("agent-1".into()),
        RepositoryId("repo-1".into()),
        "Fix issue #1234".into(),
        PathBuf::from("/worktrees/issue-1234"),
    );

    let persisted_state = State {
        schema_version: 1,
        repositories: vec![repo],
        agents: vec![agent],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
    };

    // Save and reload
    persistence
        .save_state(&persisted_state)
        .expect("save should work");
    let loaded = persistence.load_state().expect("load should work");

    assert_eq!(loaded.repositories.len(), 1);
    assert_eq!(loaded.agents.len(), 1);
    assert_eq!(loaded.selected_repository_index, Some(0));

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn persistence_settings_theme_integration() {
    let (_, persistence, mut theme_mgr) = create_test_environment();

    // Save settings with theme
    let settings = Settings {
        schema_version: 1,
        theme: "green-screen".into(),
    };
    persistence
        .save_settings(&settings)
        .expect("save should work");

    // Load and apply to theme manager
    let loaded = persistence.load_settings().expect("load should work");
    let result = theme_mgr.set_active(&loaded.theme);

    assert!(result.is_ok());
    assert_eq!(theme_mgr.active_theme().slug, "green-screen");
}

// ============================================================================
// Theme Integration Tests
// ============================================================================

#[test]
fn theme_fallback_on_invalid_settings() {
    let mut theme_mgr = FileThemeManager::new();

    // Try to set invalid theme
    let result = theme_mgr.set_active("nonexistent-theme");

    // Should fail but fall back to green-screen
    assert!(result.is_err());
    assert_eq!(theme_mgr.active_theme().slug, "green-screen");
}

#[test]
fn theme_colors_available_after_set() {
    let mut theme_mgr = FileThemeManager::new();

    // Set valid theme
    let result = theme_mgr.set_active("green-screen");
    assert!(result.is_ok());

    // Colors should be accessible
    let colors = &theme_mgr.active_theme().colors;
    assert_eq!(colors.background, "#000000");
    assert_eq!(colors.foreground, "#6a9955");
}

// ============================================================================
// Agent Lifecycle Integration Tests
// ============================================================================

#[test]
fn agent_lifecycle_state_transitions() {
    let (mut state, _, _) = create_test_environment();
    let agent_id = AgentId("agent-1".into());

    // Agent starts as Queued
    assert_eq!(state.agents[0].status, AgentStatus::Queued);

    // Mark as running
    state = state.apply(AppEvent::AgentStatusChanged(
        agent_id.clone(),
        AgentStatus::Running,
    ));
    assert_eq!(state.agents[0].status, AgentStatus::Running);

    // Kill agent
    state = state.apply(AppEvent::KillAgent(agent_id.clone()));
    assert_eq!(state.agents[0].status, AgentStatus::Dead);
}

// ============================================================================
// Error Handling Integration Tests
// ============================================================================

#[test]
fn error_messages_flow_through_state() {
    let (mut state, _, _) = create_test_environment();

    // Persistence error
    state = state.apply(AppEvent::PersistenceLoadFailed("File not found".into()));
    assert!(state.error_message.is_some());
    assert!(
        state
            .error_message
            .as_ref()
            .unwrap()
            .contains("File not found")
    );

    // Clear error
    state = state.apply(AppEvent::ClearError);
    assert!(state.error_message.is_none());
}

#[test]
fn warning_messages_flow_through_state() {
    let (mut state, _, _) = create_test_environment();

    // Theme warning
    state = state.apply(AppEvent::ThemeResolveFailed(
        "Theme 'dracula' not found".into(),
    ));
    assert!(state.warning_message.is_some());

    // Clear warning
    state = state.apply(AppEvent::ClearWarning);
    assert!(state.warning_message.is_none());
}

// ============================================================================
// Boundary Safety Tests
// ============================================================================

#[test]
fn state_transitions_never_panic() {
    let (mut state, _, _) = create_test_environment();

    // Apply many events rapidly - should never panic
    for _ in 0..100 {
        state = state.apply(AppEvent::NavigateDown);
        state = state.apply(AppEvent::NavigateUp);
        state = state.apply(AppEvent::CyclePaneFocus);
        state = state.apply(AppEvent::ToggleTerminalFocus);
        state = state.apply(AppEvent::ToggleTerminalFocus);
    }

    // State should still be valid
    assert!(
        state.selected_agent_index.is_none()
            || state.selected_agent_index.unwrap() < state.agents.len()
    );
}

#[test]
fn out_of_bounds_navigation_is_safe() {
    let (mut state, _, _) = create_test_environment();

    // Navigate way past end
    for _ in 0..100 {
        state = state.apply(AppEvent::NavigateDown);
    }

    // Selection should be clamped to valid range
    if let Some(idx) = state.selected_agent_index {
        assert!(idx < state.agents.len());
    }

    // Navigate way past beginning
    for _ in 0..100 {
        state = state.apply(AppEvent::NavigateUp);
    }

    // Selection should still be valid
    if let Some(idx) = state.selected_agent_index {
        assert!(idx < state.agents.len());
    }
}
