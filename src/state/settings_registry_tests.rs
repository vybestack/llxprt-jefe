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

use crate::domain::action_registry::ActionId;
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::workbench::descriptor::LayoutNode;

use super::AppState;
use super::agent_types_editor::AgentIntent;
use super::keys_editor_project::KeyIntent;
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
        plugin_configs: std::collections::BTreeMap::new(),
        installed_plugin_configs: std::collections::BTreeMap::new(),
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
    let mut state = AppState::test_fixture();
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

fn registry() -> crate::workbench::screens::ScreenRegistry {
    crate::workbench::builtin_screens()
        .unwrap_or_else(|error| panic!("compiled screen table: {error}"))
}

#[test]
fn moving_a_screen_writes_one_order_array_holding_every_enabled_screen_once() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let rows = project_screens(&registry(), &published(&state));
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
    let rows = project_screens(&registry(), &published(&state));
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
    let rows = project_screens(&registry(), &published(&state));
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
    let rows = project_screens(&registry(), &published(&state));
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
    let rows = project_screens(&registry(), &published(&state));
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
    let registry = registry();
    let descriptor = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));

    screen_intent(
        &mut state,
        ScreenIntent::ReplaceLayout {
            screen_id: screen(descriptor.id.as_str()),
            // A tree the descriptor validator accepts, so what is proved here
            // is the write path rather than the refusal path.
            layout: Box::new(descriptor.layout.clone()),
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
    let registry = registry();
    let descriptor = registry
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
fn a_layout_override_the_validator_refuses_blocks_the_save_and_keeps_the_draft() {
    let mut state = opened(
        b"settings_schema = 2
",
    );
    let registry = registry();
    let descriptor = registry
        .screens()
        .iter()
        .find(|screen| screen.panels.len() >= 2)
        .unwrap_or_else(|| panic!("a screen with two panels"));
    let panel = descriptor.panels[0].id;

    // One leaf leaves every other declared panel unplaced, which is exactly
    // what the descriptor validator refuses.
    screen_intent(
        &mut state,
        ScreenIntent::ReplaceLayout {
            screen_id: screen(descriptor.id.as_str()),
            layout: Box::new(LayoutNode::Leaf { panel }),
        },
    );

    assert!(state.settings_state.is_dirty(), "the work is kept");
    let diagnostics = super::settings_view::diagnostics(&state.settings_state);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.redacted_detail.contains("SCR-E301")),
        "the descriptor validator's refusal is reported: {diagnostics:?}"
    );

    state.reduce_settings(SettingsMessage::Save);
    assert!(
        state
            .settings_state
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("blocked")),
        "a candidate the validator refuses cannot be saved"
    );
}

// ── CW08-05/07/08: chords are drafted whole, and protected rows refuse ────

fn context(value: &str) -> ContextId {
    ContextId::parse(value).unwrap_or_else(|error| panic!("context fixture {value}: {error}"))
}

fn action(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("action fixture {value}: {error}"))
}

fn chord(value: &str) -> Chord {
    Chord::parse(value).unwrap_or_else(|error| panic!("chord fixture {value}: {error:?}"))
}

fn key_intent(state: &mut AppState, intent: KeyIntent) {
    state.reduce_settings(SettingsMessage::Key(Box::new(intent)));
}

/// A state with Settings open and the composed action registry committed.
fn opened_with_keys(bytes: &[u8]) -> AppState {
    // Composition happened at startup (the state's published workbench), and
    // Settings binds against it later — the fixture keeps that order.
    let mut state = AppState::test_fixture();
    state.reduce_settings(SettingsMessage::Open(Box::new(source(bytes))));
    state
}

/// Focus the Keys row for one action, the way a user reaches it.
fn focus_key_row(state: &mut AppState, action_id: &str) {
    state.reduce_settings(SettingsMessage::SelectSection(
        crate::messages::settings::SettingsSection::Keys,
    ));
    state.settings_state.focus = crate::state::SettingsFocus::Detail;
    let rows = crate::state::settings_view::detail_rows(
        &state.settings_state,
        state.settings_projection_authority(),
    );
    let index = rows
        .iter()
        .position(|row| match &row.kind {
            crate::state::settings_view::SettingsRowKind::KeyBinding { action, .. } => {
                action.as_str() == action_id
            }
            _ => false,
        })
        .unwrap_or_else(|| panic!("a Keys row for {action_id}"));
    state.settings_state.selected_row = index;
}

#[test]
fn capturing_one_chord_writes_exactly_that_one_chord() {
    let mut state = opened_with_keys(
        b"settings_schema = 2
",
    );

    key_intent(
        &mut state,
        KeyIntent::CaptureSingleChord {
            context: context("global"),
            action: action("core.open-settings"),
            chord: chord("F2"),
        },
    );

    assert_eq!(
        candidate_bytes(&state),
        r#"settings_schema = 2
[keymap."global"]
"core.open-settings" = ["F2"]
"#
    );
}

#[test]
fn setting_several_chords_writes_the_whole_list() {
    let mut state = opened_with_keys(
        b"settings_schema = 2
",
    );

    key_intent(
        &mut state,
        KeyIntent::SetChords {
            context: context("global"),
            action: action("core.open-settings"),
            chords: vec![chord("F2"), chord("Ctrl+,")],
        },
    );

    assert_eq!(
        published(&state)
            .keymap
            .get("global")
            .and_then(|actions| actions.get("core.open-settings"))
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn unbinding_writes_an_empty_list_rather_than_removing_the_assignment() {
    let mut state = opened_with_keys(
        b"settings_schema = 2
",
    );

    key_intent(
        &mut state,
        KeyIntent::Unbind {
            context: context("global"),
            action: action("core.open-settings"),
        },
    );

    assert_eq!(
        candidate_bytes(&state),
        r#"settings_schema = 2
[keymap."global"]
"core.open-settings" = []
"#
    );
}

#[test]
fn resetting_a_binding_removes_the_assignment() {
    let source = r#"settings_schema = 2
[keymap.global]
"core.open-settings" = ["F2"]
"#;
    let mut state = opened_with_keys(source.as_bytes());

    key_intent(
        &mut state,
        KeyIntent::Reset {
            context: context("global"),
            action: action("core.open-settings"),
        },
    );

    assert_eq!(
        candidate_bytes(&state),
        "settings_schema = 2\n[keymap.global]\n"
    );
}

#[test]
fn a_protected_action_refuses_every_change_with_the_registrys_own_reason() {
    for intent in [
        KeyIntent::Unbind {
            context: context("global"),
            action: action("core.emergency-exit"),
        },
        KeyIntent::Reset {
            context: context("global"),
            action: action("core.emergency-exit"),
        },
        KeyIntent::CaptureSingleChord {
            context: context("global"),
            action: action("core.emergency-exit"),
            chord: chord("F8"),
        },
    ] {
        let mut state = opened_with_keys(
            b"settings_schema = 2
",
        );

        key_intent(&mut state, intent);

        assert!(!state.settings_state.is_dirty(), "nothing was written");
        assert_eq!(
            state.settings_state.notice.as_deref(),
            Some(crate::domain::action_registry::PROTECTED_ACTION_REASON)
        );
    }
}

#[test]
fn a_key_edit_never_touches_the_active_action_registry() {
    let mut state = opened_with_keys(
        b"settings_schema = 2
",
    );
    let before = state.action_registry().clone();

    key_intent(
        &mut state,
        KeyIntent::CaptureSingleChord {
            context: context("global"),
            action: action("core.open-settings"),
            chord: chord("F2"),
        },
    );

    assert_eq!(state.action_registry(), &before);
}

#[test]
fn a_chord_the_resolver_refuses_blocks_the_save_and_keeps_the_draft() {
    let mut state = opened_with_keys(
        b"settings_schema = 2
",
    );

    key_intent(
        &mut state,
        KeyIntent::SetChords {
            context: context("global"),
            action: action("core.open-settings"),
            // The emergency exit already owns this chord in this context, and
            // it is protected, so nothing may shadow it.
            chords: vec![chord("Ctrl+Q")],
        },
    );

    assert!(state.settings_state.is_dirty(), "the work is kept");
    assert_eq!(
        published(&state)
            .keymap
            .get("global")
            .and_then(|actions| actions.get("core.open-settings"))
            .map(Vec::len),
        Some(1),
        "the refused edit stays visible, or the user is told about a conflict \
         they cannot see"
    );
    state.reduce_settings(SettingsMessage::Save);
    assert!(
        state
            .settings_state
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("blocked")),
        "a candidate the resolver refuses cannot be saved: {:?}",
        state.settings_state.notice
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

#[test]
fn a_refused_reorder_leaves_the_cursor_where_it_was() {
    let mut state = opened(b"settings_schema = 2\n");
    let rows = project_screens(&registry(), &published(&state));
    let last = rows[rows.len() - 1].screen_id.as_str().to_owned();
    state.settings_state.section = crate::messages::settings::SettingsSection::Screens;
    state.settings_state.focus = super::settings_types::SettingsFocus::Detail;
    state.settings_state.selected_row = rows.len() - 1;

    // There is no row after the last one, so nothing anchors the move.
    state.reduce_settings(SettingsMessage::ReorderRow(crate::messages::NavDir::Down));

    assert_eq!(
        state.settings_state.selected_row,
        rows.len() - 1,
        "the cursor stays on {last}"
    );
}

#[test]
fn opening_a_layout_the_grammar_cannot_read_says_so_rather_than_showing_another_tree() {
    let registry = registry();
    let descriptor = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let source = format!(
        r#"settings_schema = 2
[workbench.layout_overrides]
"{}" = {{ type = "leaf", panel = "list", nonsense = 1 }}
"#,
        descriptor.id.as_str()
    );
    let mut state = opened(source.as_bytes());

    screen_intent(
        &mut state,
        ScreenIntent::ReplaceLayout {
            screen_id: screen(descriptor.id.as_str()),
            layout: Box::new(descriptor.layout.clone()),
        },
    );
    state.settings_state.layout_editor = None;
    state.reduce_settings(SettingsMessage::Screen(Box::new(
        ScreenIntent::ResetLayout {
            screen_id: screen(descriptor.id.as_str()),
        },
    )));

    let mut fresh = opened(source.as_bytes());
    fresh.apply_settings_activation(super::settings_view::SettingsActivation::OpenLayout {
        screen_id: screen(descriptor.id.as_str()),
    });

    assert!(
        fresh.settings_state.layout_editor.is_none(),
        "an unreadable override does not open as some other tree"
    );
    assert!(
        fresh.settings_state.notice.is_some(),
        "and the reason is reported"
    );
}

#[test]
fn a_second_capture_adds_a_chord_rather_than_replacing_the_first() {
    let mut state = opened_with_keys(b"settings_schema = 2\n");

    key_intent(
        &mut state,
        KeyIntent::CaptureSingleChord {
            context: context("global"),
            action: action("core.open-settings"),
            chord: chord("F2"),
        },
    );
    focus_key_row(&mut state, "core.open-settings");
    state.reduce_settings(SettingsMessage::AddChord);
    state.reduce_settings(SettingsMessage::CapturedChord(chord("F3")));

    assert_eq!(
        published(&state)
            .keymap
            .get("global")
            .and_then(|actions| actions.get("core.open-settings"))
            .map(Vec::len),
        Some(2),
        "an action can carry more than one chord, as the registry allows"
    );
}

#[test]
fn adding_a_chord_to_an_unbound_action_binds_exactly_that_one() {
    let mut state = opened_with_keys(b"settings_schema = 2\n");

    key_intent(
        &mut state,
        KeyIntent::Unbind {
            context: context("global"),
            action: action("core.open-settings"),
        },
    );
    focus_key_row(&mut state, "core.open-settings");
    state.reduce_settings(SettingsMessage::AddChord);
    state.reduce_settings(SettingsMessage::CapturedChord(chord("F3")));

    assert_eq!(
        published(&state)
            .keymap
            .get("global")
            .and_then(|actions| actions.get("core.open-settings")),
        Some(&vec!["F3".to_owned()])
    );
}
