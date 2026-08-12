//! Behavioral tests for what the Settings screen shows and how leaving it works.
//!
//! @requirement CW07-11

use std::path::PathBuf;

use crate::domain::plugin::{ConfigSchema, Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{CanonicalSemver, Id, ThemeId};
use crate::messages::NavDir;
use crate::messages::settings::{
    PluginConfigMessage, SelectedPluginConfig, SettingsEnvironment, SettingsMessage,
    SettingsSection, SettingsSource, ThemeChoice,
};
use crate::persistence::{SettingsEdit, SettingsSaveOutcome};
use crate::workbench::ScreenId;

use super::navigation_dirty::DirtyChoice;
use super::settings_types::{DirtyChoiceCursor, SettingsFocus};
use super::{AppState, settings_view};

const SCHEMA_2: &[u8] = b"settings_schema = 2\n[appearance]\ntheme = 'green-screen'\n";

fn theme(slug: &str) -> ThemeId {
    ThemeId::parse(slug).unwrap_or_else(|error| panic!("theme fixture: {error}"))
}

fn source(bytes: Option<&[u8]>) -> SettingsSource {
    SettingsSource {
        bytes: bytes.map(<[u8]>::to_vec),
        revision: 0,
        active_theme: theme("green-screen"),
        themes: vec![
            ThemeChoice {
                id: theme("green-screen"),
                name: "Green Screen".to_owned(),
            },
            ThemeChoice {
                id: theme("dracula"),
                name: "Dracula".to_owned(),
            },
        ],
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
fn selected_required_string() -> (Id, SelectedPluginConfig) {
    let owner = Id::parse("vendor.config").unwrap_or_else(|error| panic!("owner fixture: {error}"));
    let field = Field::parse(FieldDraft {
        id: Id::parse("endpoint").unwrap_or_else(|error| panic!("field fixture: {error}")),
        label: "Endpoint".to_owned(),
        description: Some("Service endpoint".to_owned()),
        kind: FieldKind::String,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::Provider,
    })
    .unwrap_or_else(|error| panic!("field declaration fixture: {error}"));
    let schema = ConfigSchema::parse(1, vec![field])
        .unwrap_or_else(|error| panic!("schema fixture: {error}"));
    let version =
        CanonicalSemver::parse("1.0.0").unwrap_or_else(|error| panic!("version fixture: {error}"));
    (
        owner,
        SelectedPluginConfig {
            version,
            schema,
            can_migrate: true,
        },
    )
}

/// A state with Settings open over `bytes`.
fn opened(bytes: Option<&[u8]>) -> AppState {
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(source(bytes))));
    state
}
#[test]
fn active_selected_plugin_config_projects_generated_row_and_adjacent_error() {
    let bytes =
        b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n";
    let mut input = source(Some(bytes));
    let (owner, selected) = selected_required_string();
    input.plugin_configs.insert(owner.clone(), selected.clone());
    input.installed_plugin_configs.insert(owner, vec![selected]);
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));
    state.settings_state.section = SettingsSection::Plugins;

    let rows = settings_view::detail_rows(&state.settings_state);
    let generated = rows
        .iter()
        .find(|row| row.label == "vendor.config / Endpoint")
        .unwrap_or_else(|| panic!("selected package schema projects its field"));
    assert!(generated.value.contains("<unset>"));
    assert!(generated.value.contains("error: required"));
    assert!(
        generated.activation().is_some(),
        "generated fields are editable"
    );
}

#[test]
fn changing_package_version_projects_the_exact_installed_target_schema() {
    let bytes = b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n[plugins.\"vendor.config\".config]\nendpoint = \"https://example.test\"\n";
    let mut input = source(Some(bytes));
    let (owner, source) = selected_required_string();
    let target_field = Field::parse(FieldDraft {
        id: Id::parse("region").unwrap_or_else(|error| panic!("field fixture: {error}")),
        label: "Region".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::Provider,
    })
    .unwrap_or_else(|error| panic!("field declaration fixture: {error}"));
    let target = SelectedPluginConfig {
        version: CanonicalSemver::parse("2.0.0")
            .unwrap_or_else(|error| panic!("version fixture: {error}")),
        schema: ConfigSchema::parse(2, vec![target_field])
            .unwrap_or_else(|error| panic!("schema fixture: {error}")),
        can_migrate: true,
    };
    input.plugin_configs.insert(owner.clone(), source.clone());
    input
        .installed_plugin_configs
        .insert(owner.clone(), vec![target.clone(), source]);
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));
    state.settings_state.section = SettingsSection::Plugins;

    state.reduce_settings(SettingsMessage::Edit(SettingsEdit::PluginVersion {
        plugin: owner,
        version: target.version,
    }));

    let rows = settings_view::detail_rows(&state.settings_state);
    assert!(
        rows.iter().any(|row| row.label == "vendor.config / Region"),
        "the draft-selected target schema owns generated fields"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.label == "vendor.config / Endpoint"),
        "the startup-selected source schema no longer owns the draft"
    );
}

#[test]
fn generated_string_field_edits_through_the_settings_owned_editor() {
    let bytes =
        b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n";
    let mut input = source(Some(bytes));
    let (owner, selected) = selected_required_string();
    input.plugin_configs.insert(owner.clone(), selected.clone());
    input.installed_plugin_configs.insert(owner, vec![selected]);
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));
    state.settings_state.section = SettingsSection::Plugins;
    state.settings_state.focus = SettingsFocus::Detail;
    let rows = settings_view::detail_rows(&state.settings_state);
    state.settings_state.selected_row = rows
        .iter()
        .position(|row| row.label == "vendor.config / Endpoint")
        .unwrap_or_else(|| panic!("generated endpoint row"));

    state.reduce_settings(SettingsMessage::Activate);
    for character in "https://example.test".chars() {
        state.reduce_settings(SettingsMessage::PluginConfig(
            PluginConfigMessage::TypeChar(character),
        ));
    }
    state.reduce_settings(SettingsMessage::PluginConfig(PluginConfigMessage::Apply));

    let candidate = state
        .settings_state
        .draft
        .as_ref()
        .unwrap_or_else(|| panic!("settings draft"))
        .candidate();
    let valid = candidate
        .valid()
        .unwrap_or_else(|| panic!("edited config candidate is valid"));
    let text = std::str::from_utf8(valid.bytes())
        .unwrap_or_else(|error| panic!("candidate utf8: {error}"));
    assert!(
        text.contains("\"endpoint\" = \"https://example.test\""),
        "candidate was:\n{text}"
    );
    assert!(state.settings_state.plugin_config_editor.is_none());
}

#[test]
fn hidden_generated_plugin_config_fields_are_omitted() {
    let bytes = b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n[plugins.\"vendor.config\".config]\nshow = false\n";
    let show = Field::parse(FieldDraft {
        id: Id::parse("show").unwrap_or_else(|error| panic!("field fixture: {error}")),
        label: "Show".to_owned(),
        description: None,
        kind: FieldKind::Boolean,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field declaration fixture: {error}"));
    let detail = Field::parse(FieldDraft {
        id: Id::parse("detail").unwrap_or_else(|error| panic!("field fixture: {error}")),
        label: "Detail".to_owned(),
        description: None,
        kind: FieldKind::String,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: Some(show.id().clone()),
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("field declaration fixture: {error}"));
    let schema = ConfigSchema::parse(1, vec![show, detail])
        .unwrap_or_else(|error| panic!("schema fixture: {error}"));
    let mut input = source(Some(bytes));
    let owner = Id::parse("vendor.config").unwrap_or_else(|error| panic!("owner: {error}"));
    let version =
        CanonicalSemver::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"));
    let selected = SelectedPluginConfig {
        version,
        schema,
        can_migrate: true,
    };
    input.plugin_configs.insert(owner.clone(), selected.clone());
    input.installed_plugin_configs.insert(owner, vec![selected]);
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));
    state.settings_state.section = SettingsSection::Plugins;
    assert!(
        !settings_view::detail_rows(&state.settings_state)
            .iter()
            .any(|row| row.label.ends_with(" / Detail"))
    );
}

fn apply(state: &mut AppState, message: SettingsMessage) {
    state.reduce_settings(message);
}

/// Answer the scheduled save as the writer would after a successful write.
fn complete_written(state: &mut AppState) {
    let revision = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::pending_revision)
        .unwrap_or_else(|| panic!("a scheduled save carries a revision"));
    let Some(candidate) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::candidate_bytes)
    else {
        panic!("a saveable draft has a candidate");
    };
    apply(
        state,
        SettingsMessage::SaveCompleted(Box::new(SettingsSaveOutcome::Written {
            revision,
            hash: candidate.sha256(),
        })),
    );
}

// ── CW07-11: the screen's own states ─────────────────────────────────────

#[test]
fn the_sections_run_from_the_session_outwards_to_its_registries_and_problems() {
    let state = opened(Some(SCHEMA_2));

    let titles = settings_view::section_rows(&state.settings_state)
        .into_iter()
        .map(|row| row.title)
        .collect::<Vec<_>>();

    assert_eq!(
        titles,
        vec![
            "General",
            "Appearance",
            "Agent Types",
            "Screens",
            "Keys",
            "Plugins",
            "Diagnostics",
        ]
    );
}

#[test]
fn the_diagnostics_section_carries_its_count() {
    let state = opened(Some(b"settings_schema = 2\n[appearance]\ntheme = 42\n"));

    let rows = settings_view::section_rows(&state.settings_state);
    let Some(diagnostics) = rows
        .iter()
        .find(|row| row.section == SettingsSection::Diagnostics)
    else {
        panic!("the diagnostics section is always listed");
    };
    assert_eq!(diagnostics.count, Some(1));
}

#[test]
fn focus_moves_between_the_section_list_and_the_detail_pane() {
    let mut state = opened(Some(SCHEMA_2));
    assert_eq!(state.settings_state.focus, SettingsFocus::Sections);

    apply(&mut state, SettingsMessage::CycleFocus);
    assert_eq!(state.settings_state.focus, SettingsFocus::Detail);

    apply(&mut state, SettingsMessage::CycleFocusReverse);
    assert_eq!(state.settings_state.focus, SettingsFocus::Sections);
}

#[test]
fn moving_the_section_selection_changes_which_section_is_shown() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Navigate(NavDir::Down));

    assert_eq!(state.settings_state.section, SettingsSection::Appearance);
    assert_eq!(state.settings_state.selected_row, 1);
}

#[test]
fn the_appearance_rows_carry_every_installed_theme_and_the_override_toggle() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::SelectSection(SettingsSection::Appearance),
    );

    let rows = settings_view::detail_rows(&state.settings_state);

    assert!(rows.iter().any(|row| row.label == "Green Screen"));
    assert!(rows.iter().any(|row| row.label == "Dracula"));
    assert!(rows.iter().any(|row| row.label == "Apply theme to agent"));
}

#[test]
fn a_theme_the_document_names_but_the_manager_cannot_resolve_renders_unavailable() {
    let mut state = opened(Some(
        b"settings_schema = 2\n[appearance]\ntheme = 'missing-theme'\n",
    ));
    apply(
        &mut state,
        SettingsMessage::SelectSection(SettingsSection::Appearance),
    );

    let rows = settings_view::detail_rows(&state.settings_state);

    let Some(row) = rows.iter().find(|row| row.label == "missing-theme") else {
        panic!("an unresolvable theme is still shown: {rows:?}");
    };
    assert_eq!(row.value, "unavailable: not installed");
    assert!(
        row.activation().is_none(),
        "an uninstalled theme cannot be selected"
    );
}

#[test]
fn activating_a_theme_row_edits_the_draft() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::SelectSection(SettingsSection::Appearance),
    );
    apply(&mut state, SettingsMessage::CycleFocus);
    apply(&mut state, SettingsMessage::Navigate(NavDir::Down));

    apply(&mut state, SettingsMessage::Activate);

    assert_eq!(
        state
            .settings_state
            .draft
            .as_ref()
            .and_then(|draft| draft.published().appearance.theme.clone())
            .as_deref(),
        Some("dracula")
    );
}

#[test]
fn the_general_section_reports_the_paths_and_platform_it_is_using() {
    let state = opened(Some(SCHEMA_2));

    let rows = settings_view::detail_rows(&state.settings_state);

    assert!(rows.iter().any(|row| row.label == "Settings"));
    assert!(rows.iter().any(|row| row.label == "State"));
    assert!(rows.iter().any(|row| row.label == "Platform"));
    assert!(
        rows.iter()
            .any(|row| row.label == "Isolated" && row.value == "yes")
    );
}

#[test]
fn the_diagnostics_section_is_read_only() {
    let mut state = opened(Some(b"settings_schema = 2\n[appearance]\ntheme = 42\n"));
    apply(
        &mut state,
        SettingsMessage::SelectSection(SettingsSection::Diagnostics),
    );

    let rows = settings_view::detail_rows(&state.settings_state);

    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row.activation().is_none()));
    assert!(rows.iter().all(|row| row.toggle().is_none()));
    assert!(rows.iter().all(|row| row.reset().is_none()));
    assert!(rows.iter().all(|row| row.unbind().is_none()));
}

#[test]
fn a_structural_save_says_exactly_what_the_user_must_do() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::InitialScreen(
            crate::domain::Id::parse("core.errors")
                .unwrap_or_else(|error| panic!("screen id fixture: {error}")),
        )),
    );
    apply(&mut state, SettingsMessage::Save);

    complete_written(&mut state);

    assert_eq!(
        state.settings_state.notice.as_deref(),
        Some(super::settings::RESTART_NOTICE)
    );
}

// ── The host dirty guard ─────────────────────────────────────────────────

#[test]
fn leaving_a_clean_settings_screen_releases_the_draft() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Back);

    assert!(!state.settings_state.active);
    assert!(state.settings_state.draft.is_none());
    assert_ne!(state.screen(), ScreenId::Settings);
}

#[test]
fn leaving_a_dirty_settings_screen_is_held_back_until_it_is_answered() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Back);

    assert_eq!(state.screen(), ScreenId::Settings);
    assert!(state.nav.guard().is_some(), "the guard is up");
    assert!(state.settings_state.is_dirty(), "the draft is untouched");
}

#[test]
fn cancelling_the_guard_keeps_the_draft_and_the_screen() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);

    apply(
        &mut state,
        SettingsMessage::ResolveDirty(DirtyChoice::Cancel),
    );

    assert_eq!(state.screen(), ScreenId::Settings);
    assert!(state.settings_state.is_dirty());
    assert!(state.nav.guard().is_none());
}

#[test]
fn discarding_at_the_guard_abandons_the_draft_and_leaves() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);

    apply(
        &mut state,
        SettingsMessage::ResolveDirty(DirtyChoice::Discard),
    );

    assert_ne!(state.screen(), ScreenId::Settings);
    assert!(!state.settings_state.active);
}

#[test]
fn saving_at_the_guard_keeps_the_screen_until_the_save_succeeds() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);

    apply(&mut state, SettingsMessage::ResolveDirty(DirtyChoice::Save));

    assert_eq!(
        state.screen(),
        ScreenId::Settings,
        "the save is still in flight"
    );
    assert!(
        state
            .settings_state
            .draft
            .as_ref()
            .is_some_and(|draft| draft.status().is_saving())
    );
}

#[test]
fn a_guard_save_that_succeeds_leaves_the_screen_and_releases_the_draft() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);
    apply(&mut state, SettingsMessage::ResolveDirty(DirtyChoice::Save));

    complete_written(&mut state);

    assert_ne!(state.screen(), ScreenId::Settings, "the guard let go");
    assert!(state.nav.guard().is_none(), "the guard is not stuck");
    assert!(!state.settings_state.active);
    assert!(state.settings_state.draft.is_none());
}

#[test]
fn a_guard_save_that_fails_keeps_the_user_with_their_work() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);
    apply(&mut state, SettingsMessage::ResolveDirty(DirtyChoice::Save));
    let Some(revision) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::pending_revision)
    else {
        panic!("the guard's save is scheduled");
    };

    apply(
        &mut state,
        SettingsMessage::SaveCompleted(Box::new(SettingsSaveOutcome::Conflict {
            revision,
            disk_hash: None,
        })),
    );

    assert_eq!(
        state.screen(),
        ScreenId::Settings,
        "the user keeps the screen"
    );
    assert!(state.settings_state.is_dirty(), "the draft survives");
    assert!(
        matches!(
            state
                .nav
                .guard()
                .map(super::navigation_dirty::DirtyGuard::phase),
            Some(super::navigation_dirty::GuardPhase::Failed { .. })
        ),
        "the guard re-offers its choices instead of waiting forever"
    );
}

#[test]
fn a_guard_save_of_a_draft_that_cannot_be_saved_does_not_strand_the_guard() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Back);
    // The document goes bad underneath the draft, so there is nothing to save.
    apply(
        &mut state,
        SettingsMessage::Reloaded(Box::new(source(Some(
            b"settings_schema = 2
[appearance]
theme = 42
",
        )))),
    );

    apply(&mut state, SettingsMessage::ResolveDirty(DirtyChoice::Save));

    assert!(
        !matches!(
            state
                .nav
                .guard()
                .map(super::navigation_dirty::DirtyGuard::phase),
            Some(super::navigation_dirty::GuardPhase::SaveRequested { .. })
        ),
        "a save that never runs still has to be answered"
    );
}

#[test]
fn the_guard_focus_cycles_through_save_discard_and_cancel() {
    let mut state = opened(Some(SCHEMA_2));
    assert_eq!(state.settings_state.dirty_choice, DirtyChoiceCursor::Save);

    apply(&mut state, SettingsMessage::NavigateDirty(NavDir::Next));
    assert_eq!(
        state.settings_state.dirty_choice,
        DirtyChoiceCursor::Discard
    );

    apply(&mut state, SettingsMessage::NavigateDirty(NavDir::Next));
    assert_eq!(state.settings_state.dirty_choice, DirtyChoiceCursor::Cancel);

    apply(&mut state, SettingsMessage::NavigateDirty(NavDir::Next));
    assert_eq!(
        state.settings_state.dirty_choice,
        DirtyChoiceCursor::Save,
        "the focus wraps"
    );

    apply(&mut state, SettingsMessage::NavigateDirty(NavDir::Prev));
    assert_eq!(state.settings_state.dirty_choice, DirtyChoiceCursor::Cancel);
}

// ── Nothing about a draft is persisted ───────────────────────────────────

#[test]
fn no_draft_preview_or_selection_reaches_the_durable_projection() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    let Ok(durable) = super::durable_projection::to_durable_state(&state) else {
        panic!("the durable projection must succeed");
    };
    let encoded = serde_json::to_string(&durable)
        .unwrap_or_else(|error| panic!("the durable projection must encode: {error}"));

    for token in ["dracula", "settings-draft", "preview", "draft"] {
        assert!(
            !encoded.contains(token),
            "the durable document must not carry {token}: {encoded}"
        );
    }
}

#[test]
fn migration_preview_notes_are_control_safe_and_value_free() {
    let owner = Id::parse("vendor.config").unwrap_or_else(|error| panic!("owner fixture: {error}"));
    let version =
        CanonicalSemver::parse("2.0.0").unwrap_or_else(|error| panic!("version fixture: {error}"));
    let settings = super::settings_types::SettingsState {
        plugin_config_migration: super::settings_types::PluginConfigMigrationState::Preview(
            super::settings_types::PluginConfigMigrationPreview {
                request: super::settings_types::PluginConfigMigration {
                    owner,
                    source_package_version: CanonicalSemver::parse("1.0.0")
                        .unwrap_or_else(|error| panic!("version fixture: {error}")),
                    target_package_version: version,
                    from_schema_version: 1,
                    to_schema_version: 2,
                    source_config: crate::domain::TypedMap::new(),
                    draft_token: super::navigation_dirty::DraftToken::next(),
                    exit_after_save: false,
                },
                target_config: crate::domain::TypedMap::new(),
                notes: vec!["line one\n\u{1b}[31mline two".to_owned()],
                diff: vec![super::settings_types::PluginConfigDiffRow {
                    path: "/plugins/vendor.config/config/region".to_owned(),
                    change: super::settings_types::PluginConfigChange::Changed,
                }],
            },
        ),
        ..super::settings_types::SettingsState::default()
    };

    let Some(settings_view::PluginConfigMigrationView::Preview { changes, notes, .. }) =
        settings_view::plugin_config_migration_view(&settings)
    else {
        panic!("preview state projects a migration view");
    };
    assert_eq!(
        changes,
        vec!["/plugins/vendor.config/config/region: changed"]
    );
    assert_eq!(notes, vec!["line one  [31mline two"]);
    assert!(!notes[0].contains('\u{1b}'));
}
