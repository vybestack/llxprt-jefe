//! Behavioral tests for the registry-editor intents the Settings reducer
//! accepts (issue #388).
//!
//! @requirement CW08-01
//! @requirement CW08-02
//! @requirement CW08-03
//! @requirement CW08-07

use std::path::PathBuf;

use crate::domain::ThemeId;
use crate::domain::agent_definition::AgentTypeId;
use crate::messages::settings::{
    SettingsEnvironment, SettingsMessage, SettingsSource, ThemeChoice,
};
use crate::persistence::settings_document::PublishedSettings;

use super::AppState;
use super::agent_types_editor::AgentIntent;
use super::settings_types::SettingsDraft;

fn theme(slug: &str) -> ThemeId {
    ThemeId::parse(slug).unwrap_or_else(|error| panic!("theme fixture: {error}"))
}

fn agent(id: &str) -> AgentTypeId {
    AgentTypeId::parse(id).unwrap_or_else(|error| panic!("agent id fixture {id}: {error}"))
}

fn source(bytes: &[u8]) -> SettingsSource {
    SettingsSource {
        bytes: Some(bytes.to_vec()),
        revision: 0,
        active_theme: theme("green-screen"),
        themes: vec![ThemeChoice {
            id: theme("green-screen"),
            name: "Green Screen".to_owned(),
        }],
        environment: SettingsEnvironment {
            settings_path: PathBuf::from("/tmp/jefe/settings.toml"),
            state_path: PathBuf::from("/tmp/jefe/state.json"),
            platform: "test",
            isolated: true,
        },
    }
}

/// A state with Settings open over `bytes`.
fn opened(bytes: &[u8]) -> AppState {
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(source(bytes))));
    state
}

/// The exact bytes the draft would save.
fn candidate_bytes(state: &AppState) -> String {
    let Some(candidate) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(SettingsDraft::candidate_bytes)
    else {
        panic!("the draft must hold a valid candidate");
    };
    String::from_utf8_lossy(candidate.bytes()).into_owned()
}

/// The typed settings the draft currently describes.
fn published(state: &AppState) -> PublishedSettings {
    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a draft must be bound");
    };
    draft.published().clone()
}

// ── CW08-01: agent enablement is drafted sparsely and applies on restart ───

#[test]
fn disabling_an_agent_type_writes_only_its_own_enabled_path() {
    let mut state = opened(b"settings_schema = 2\n");

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: false,
    }));

    assert_eq!(
        candidate_bytes(&state),
        "settings_schema = 2\n[agents.\"core.llxprt\"]\nenabled = false\n"
    );
    assert!(state.settings_state.is_dirty());
}

#[test]
fn an_agent_toggle_leaves_every_other_agents_syntax_alone() {
    let mut state = opened(b"settings_schema = 2\n[agents.\"core.codex\"]\nenabled = false\n");

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: false,
    }));

    assert_eq!(
        candidate_bytes(&state),
        "settings_schema = 2\n[agents.\"core.codex\"]\nenabled = false\n[agents.\"core.llxprt\"]\nenabled = false\n"
    );
}

#[test]
fn resetting_an_agent_type_returns_it_to_its_compiled_default() {
    let mut state = opened(b"settings_schema = 2\n[agents.\"core.llxprt\"]\nenabled = false\n");

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::Reset {
        type_id: agent("core.llxprt"),
    }));

    let Ok(owner) = crate::domain::Id::parse("core.llxprt") else {
        panic!("owner id fixture");
    };
    assert_eq!(
        published(&state).agents.get(&owner).and_then(|o| o.enabled),
        None,
        "no assignment means the compiled default stands"
    );
    assert!(state.settings_state.is_dirty());
}

#[test]
fn toggling_an_agent_type_back_to_what_the_file_says_leaves_no_unsaved_work() {
    let mut state = opened(b"settings_schema = 2\n[agents.\"core.llxprt\"]\nenabled = false\n");

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: true,
    }));
    assert!(state.settings_state.is_dirty());

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: false,
    }));

    assert!(
        !state.settings_state.is_dirty(),
        "an edit that puts the document back is not unsaved work"
    );
}

#[test]
fn an_agent_toggle_never_touches_the_active_agent_registry() {
    let mut state = opened(b"settings_schema = 2\n");
    let before = state.agent_type_availability.clone();
    let available_before = state.available_agent_type_ids.clone();

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: false,
    }));

    assert_eq!(state.agent_type_availability, before);
    assert_eq!(state.available_agent_type_ids, available_before);
}

#[test]
fn saving_an_agent_toggle_says_a_restart_is_needed() {
    let mut state = opened(b"settings_schema = 2\n");

    state.reduce_settings(SettingsMessage::Agent(AgentIntent::SetEnabled {
        type_id: agent("core.llxprt"),
        enabled: false,
    }));

    let Some(candidate) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(SettingsDraft::candidate_bytes)
    else {
        panic!("the draft must hold a valid candidate");
    };
    assert!(
        candidate.structural(),
        "composing the agent registry happens once, at startup"
    );
}
