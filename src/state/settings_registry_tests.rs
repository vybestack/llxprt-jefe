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

use crate::workbench::descriptor::LayoutNode;

use super::AppState;
use super::agent_types_editor::AgentIntent;
use super::screens_editor::{COMPILED_MEMBERSHIP_REASON, ScreenIntent, project_screens};
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

// ── CW08-02: screen membership and order stay consistent ──────────────────

fn screen(value: &str) -> crate::domain::Id {
    crate::domain::Id::parse(value)
        .unwrap_or_else(|error| panic!("screen id fixture {value}: {error}"))
}

fn screen_intent(state: &mut AppState, intent: ScreenIntent) {
    state.reduce_settings(SettingsMessage::Screen(Box::new(intent)));
}

fn registry() -> &'static crate::workbench::screens::ScreenRegistry {
    crate::workbench::screen_registry()
        .unwrap_or_else(|error| panic!("compiled screen table: {error}"))
}

#[test]
fn moving_a_screen_writes_one_order_array_holding_every_enabled_screen_once() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(registry(), &published(&state));
    let first = rows[0].screen_id.as_str().to_owned();
    let second = rows[1].screen_id.as_str().to_owned();

    screen_intent(
        &mut state,
        ScreenIntent::MoveAfter {
            screen_id: screen(&first),
            anchor: screen(&second),
        },
    );

    let order = published(&state).workbench.screen_order;
    assert_eq!(
        order.first().map(crate::domain::Id::as_str),
        Some(second.as_str()),
        "the anchor now leads"
    );
    assert_eq!(
        order.get(1).map(crate::domain::Id::as_str),
        Some(first.as_str()),
        "the moved screen lands behind it"
    );
    let mut unique = order.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        order.len(),
        "each screen appears exactly once"
    );
    assert_eq!(
        order.len(),
        registry().screens().len(),
        "no enabled screen is dropped by a move"
    );
}

#[test]
fn moving_a_screen_before_the_leader_puts_it_at_the_head() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(registry(), &published(&state));
    let first = rows[0].screen_id.as_str().to_owned();
    let last = rows[rows.len() - 1].screen_id.as_str().to_owned();

    screen_intent(
        &mut state,
        ScreenIntent::MoveBefore {
            screen_id: screen(&last),
            anchor: screen(&first),
        },
    );

    assert_eq!(
        published(&state)
            .workbench
            .screen_order
            .first()
            .map(crate::domain::Id::as_str),
        Some(last.as_str())
    );
}

#[test]
fn moving_a_screen_onto_itself_is_not_unsaved_work() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(registry(), &published(&state));
    let only = rows[0].screen_id.as_str().to_owned();

    screen_intent(
        &mut state,
        ScreenIntent::MoveAfter {
            screen_id: screen(&only),
            anchor: screen(&only),
        },
    );

    assert!(!state.settings_state.is_dirty());
}

#[test]
fn moving_a_screen_that_is_not_enabled_says_so_and_changes_nothing() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(registry(), &published(&state));
    let anchor = rows[0].screen_id.as_str().to_owned();

    screen_intent(
        &mut state,
        ScreenIntent::MoveBefore {
            screen_id: screen("local.nothing-declares-me"),
            anchor: screen(&anchor),
        },
    );

    assert!(!state.settings_state.is_dirty());
    assert!(
        state
            .settings_state
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("local.nothing-declares-me")),
        "the user is told which screen could not be moved"
    );
}

#[test]
fn a_compiled_screen_cannot_be_turned_off_and_says_why() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(registry(), &published(&state));
    let compiled = rows[0].screen_id.as_str().to_owned();

    screen_intent(
        &mut state,
        ScreenIntent::SetEnabled {
            screen_id: screen(&compiled),
            enabled: false,
        },
    );

    assert!(!state.settings_state.is_dirty());
    assert_eq!(
        state.settings_state.notice.as_deref(),
        Some(COMPILED_MEMBERSHIP_REASON)
    );
}

// ── CW08-03: a layout override is one whole tree, and previews nothing live ─

#[test]
fn replacing_a_layout_writes_one_override_and_leaves_active_geometry_alone() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let before = state.resolved_layout.clone();
    let descriptor = registry()
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let panel = descriptor
        .panels
        .first()
        .unwrap_or_else(|| panic!("a declared panel"));

    screen_intent(
        &mut state,
        ScreenIntent::ReplaceLayout {
            screen_id: screen(descriptor.id.as_str()),
            layout: Box::new(LayoutNode::Leaf { panel: panel.id }),
        },
    );

    assert!(
        published(&state)
            .workbench
            .layout_overrides
            .contains_key(&screen(descriptor.id.as_str())),
        "the override reaches the candidate"
    );
    assert_eq!(
        state.resolved_layout, before,
        "editing a layout does not move the screen it is edited on"
    );
}

#[test]
fn resetting_a_layout_removes_the_whole_override() {
    let descriptor = registry()
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let panel = descriptor
        .panels
        .first()
        .unwrap_or_else(|| panic!("a declared panel"));
    let source = format!(
        r#"settings_schema = 2
[workbench.layout_overrides]
"{}" = {{ type = "leaf", panel = "{}" }}
"#,
        descriptor.id.as_str(),
        panel.id.as_str()
    );
    let mut state = opened(source.as_bytes());

    screen_intent(
        &mut state,
        ScreenIntent::ResetLayout {
            screen_id: screen(descriptor.id.as_str()),
        },
    );

    assert!(
        !published(&state)
            .workbench
            .layout_overrides
            .contains_key(&screen(descriptor.id.as_str()))
    );
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
