//! Behavioral tests for the Settings shell's draft, save, and recovery authority.
//!
//! @requirement CW07-01
//! @requirement CW07-02
//! @requirement CW07-03
//! @requirement CW07-05
//! @requirement CW07-06
//! @requirement CW07-07
//! @requirement CW07-08
//! @requirement CW07-09

use std::path::PathBuf;

use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::sha256::Sha256;
use crate::domain::{CanonicalSemver, Id, ThemeId, TypedMap, TypedValue};
use crate::messages::settings::{
    RecoveryChoice, SelectedPluginConfig, SettingsEnvironment, SettingsMessage, SettingsSource,
    ThemeChoice,
};
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::{SettingsEdit, SettingsSaveOutcome, SyntaxPath};
use crate::workbench::ScreenId;

use super::settings_types::{DraftStatus, PluginConfigMigrationState};
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

fn source_with_selected_config(bytes: &[u8]) -> SettingsSource {
    let mut source = source(Some(bytes));
    let (owner, selected) = selected_required_string();
    source
        .plugin_configs
        .insert(owner.clone(), selected.clone());
    source
        .installed_plugin_configs
        .insert(owner, vec![selected]);
    source
}

fn opened_migration_draft() -> (AppState, Id, CanonicalSemver) {
    let bytes = b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n[plugins.\"vendor.config\".config]\nendpoint = \"https://example.test\"\n";
    let mut input = source_with_selected_config(bytes);
    let owner = Id::parse("vendor.config").unwrap_or_else(|error| panic!("owner fixture: {error}"));
    let region = Field::parse(FieldDraft {
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
    let target_version =
        CanonicalSemver::parse("2.0.0").unwrap_or_else(|error| panic!("version fixture: {error}"));
    let target = SelectedPluginConfig {
        version: target_version.clone(),
        schema: ConfigSchema::parse(2, vec![region])
            .unwrap_or_else(|error| panic!("schema fixture: {error}")),
        can_migrate: true,
    };
    input
        .installed_plugin_configs
        .entry(owner.clone())
        .or_default()
        .insert(0, target);
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));
    state.reduce_settings(SettingsMessage::Edit(SettingsEdit::PluginVersion {
        plugin: owner.clone(),
        version: target_version.clone(),
    }));
    (state, owner, target_version)
}

/// A state with Settings open over `bytes`.
fn opened(bytes: Option<&[u8]>) -> AppState {
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(source(bytes))));
    state
}

fn apply(state: &mut AppState, message: SettingsMessage) {
    state.reduce_settings(message);
}

fn draft_status(state: &AppState) -> DraftStatus {
    state
        .settings_state
        .draft
        .as_ref()
        .map_or(DraftStatus::Clean, |draft| draft.status().clone())
}

#[test]
fn selected_active_plugin_schema_blocks_save_when_required_config_is_missing() {
    let bytes =
        b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n";
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(
        source_with_selected_config(bytes),
    )));

    let draft = state
        .settings_state
        .draft
        .as_ref()
        .unwrap_or_else(|| panic!("selected config binds a draft"));
    assert!(draft.candidate().valid().is_none());
    assert!(!draft.validation().is_empty());
}

#[test]
fn changing_package_version_validates_against_the_exact_installed_target_schema() {
    let bytes = b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = true\nversion = \"1.0.0\"\n[plugins.\"vendor.config\".config]\nendpoint = \"https://example.test\"\n";
    let mut input = source_with_selected_config(bytes);
    let owner = Id::parse("vendor.config").unwrap_or_else(|error| panic!("owner fixture: {error}"));
    let region = Field::parse(FieldDraft {
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
        schema: ConfigSchema::parse(2, vec![region])
            .unwrap_or_else(|error| panic!("schema fixture: {error}")),
        can_migrate: true,
    };
    input
        .installed_plugin_configs
        .entry(owner.clone())
        .or_default()
        .insert(0, target.clone());
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(input)));

    state.reduce_settings(SettingsMessage::Edit(SettingsEdit::PluginVersion {
        plugin: owner,
        version: target.version,
    }));

    let draft = state
        .settings_state
        .draft
        .as_ref()
        .unwrap_or_else(|| panic!("target version keeps the draft"));
    assert!(draft.candidate().valid().is_none());
    assert!(
        draft.validation().iter().any(|diagnostic| {
            diagnostic.path.as_str() == "/plugins/vendor.config/config/region"
        })
    );
}

#[test]
fn disabled_plugin_config_is_dormant_and_preserved_without_owner_validation() {
    let bytes = b"settings_schema = 2\n[plugins.\"vendor.config\"]\nenabled = false\nversion = \"1.0.0\"\n[plugins.\"vendor.config\".config]\nendpoint = 42\n";
    let mut state = AppState::default();
    state.reduce_settings(SettingsMessage::Open(Box::new(
        source_with_selected_config(bytes),
    )));

    assert_eq!(draft_status(&state), DraftStatus::Clean);
    let draft = state
        .settings_state
        .draft
        .as_ref()
        .unwrap_or_else(|| panic!("dormant plugin config stays editable"));
    assert_eq!(draft.base().document().original_bytes(), bytes);
}

#[test]
fn save_detects_schema_migration_before_target_schema_validation() {
    let (mut state, owner, target_version) = opened_migration_draft();

    apply(&mut state, SettingsMessage::Save);

    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("schema change starts a provisional migration"));
    assert_eq!(pending.owner, owner);
    assert_eq!(pending.source_package_version.as_str(), "1.0.0");
    assert_eq!(pending.target_package_version, target_version);
    assert_eq!(pending.from_schema_version, 1);
    assert_eq!(pending.to_schema_version, 2);
    assert_eq!(
        pending
            .source_config
            .get(&Id::parse("endpoint").unwrap_or_else(|error| panic!("field fixture: {error}"))),
        Some(&TypedValue::String("https://example.test".to_owned()))
    );
    assert!(!matches!(draft_status(&state), DraftStatus::Saving { .. }));
}

#[test]
fn provider_free_target_never_stages_a_config_migration() {
    let (mut state, owner, target_version) = opened_migration_draft();
    let Some(installed) = state
        .settings_state
        .installed_plugin_configs
        .get_mut(&owner)
    else {
        panic!("installed target fixture");
    };
    let Some(target) = installed
        .iter_mut()
        .find(|selected| selected.version == target_version)
    else {
        panic!("target fixture");
    };
    target.can_migrate = false;

    apply(&mut state, SettingsMessage::Save);
    assert!(matches!(
        state.settings_state.plugin_config_migration,
        PluginConfigMigrationState::Idle
    ));
    assert!(
        state
            .settings_state
            .draft
            .as_ref()
            .is_some_and(|draft| draft.pending_revision().is_none())
    );
}

#[test]
fn migration_completion_builds_owner_qualified_value_free_preview() {
    let (mut state, owner, _) = opened_migration_draft();
    apply(&mut state, SettingsMessage::Save);
    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("migration is pending"));
    let mut target_config = TypedMap::new();
    target_config.insert(
        Id::parse("region").unwrap_or_else(|error| panic!("field fixture: {error}")),
        TypedValue::String("us-east".to_owned()),
    );

    apply(
        &mut state,
        SettingsMessage::MigrationCompleted {
            draft_token: pending.draft_token.get(),
            target_config,
            notes: vec!["updated endpoint".to_owned()],
        },
    );

    let PluginConfigMigrationState::Preview(preview) =
        &state.settings_state.plugin_config_migration
    else {
        panic!("valid completion produces a preview");
    };
    assert_eq!(preview.request.owner, owner);
    assert_eq!(preview.diff.len(), 2);
    assert_eq!(
        preview
            .diff
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "/plugins/vendor.config/config/endpoint",
            "/plugins/vendor.config/config/region",
        ]
    );
    assert!(preview.diff.iter().all(|row| !row.path.contains("us-east")));
}

#[test]
fn approving_migration_replaces_owner_config_then_schedules_existing_writer() {
    let (mut state, _, _) = opened_migration_draft();
    apply(&mut state, SettingsMessage::Save);
    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("migration is pending"));
    let mut target_config = TypedMap::new();
    target_config.insert(
        Id::parse("region").unwrap_or_else(|error| panic!("field fixture: {error}")),
        TypedValue::String("us-east".to_owned()),
    );
    apply(
        &mut state,
        SettingsMessage::MigrationCompleted {
            draft_token: pending.draft_token.get(),
            target_config,
            notes: Vec::new(),
        },
    );

    apply(&mut state, SettingsMessage::ApproveMigration);

    assert!(matches!(draft_status(&state), DraftStatus::Saving { .. }));
    let candidate = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::settings_types::SettingsDraft::candidate_bytes)
        .unwrap_or_else(|| panic!("approval schedules one authoritative write"));
    let text = String::from_utf8(candidate.bytes().to_vec())
        .unwrap_or_else(|error| panic!("candidate utf8: {error}"));
    assert!(text.contains("\"region\" = \"us-east\""), "{text}");
    assert!(!text.contains("\"endpoint\" ="), "{text}");
}

#[test]
fn cancelling_migration_preserves_exact_base_and_schedules_no_write() {
    let (mut state, _, _) = opened_migration_draft();
    let base = state.settings_state.draft.as_ref().map_or_else(
        || panic!("migration draft has a base"),
        |draft| draft.base().document().original_bytes().to_vec(),
    );
    apply(&mut state, SettingsMessage::Save);

    apply(&mut state, SettingsMessage::CancelMigration);

    assert!(matches!(
        state.settings_state.plugin_config_migration,
        PluginConfigMigrationState::Idle
    ));
    assert!(!matches!(draft_status(&state), DraftStatus::Saving { .. }));
    assert_eq!(
        state
            .settings_state
            .draft
            .as_ref()
            .map(|draft| draft.base().document().original_bytes()),
        Some(base.as_slice())
    );
}

#[test]
fn migration_failure_preserves_exact_base_and_schedules_no_write() {
    let (mut state, owner, _) = opened_migration_draft();
    let base = state.settings_state.draft.as_ref().map_or_else(
        || panic!("migration draft has a base"),
        |draft| draft.base().document().original_bytes().to_vec(),
    );
    apply(&mut state, SettingsMessage::Save);
    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("migration is pending"));

    apply(
        &mut state,
        SettingsMessage::MigrationFailed {
            draft_token: pending.draft_token.get(),
            detail: "migration provider failed".to_owned(),
        },
    );

    assert!(matches!(
        &state.settings_state.plugin_config_migration,
        PluginConfigMigrationState::Failed {
            owner: failed_owner,
            detail,
        } if failed_owner == &owner && detail == "migration provider failed"
    ));
    assert!(!matches!(draft_status(&state), DraftStatus::Saving { .. }));
    assert_eq!(
        state
            .settings_state
            .draft
            .as_ref()
            .map(|draft| draft.base().document().original_bytes()),
        Some(base.as_slice())
    );
}

#[test]
fn editing_while_migration_runs_invalidates_the_completion() {
    let (mut state, owner, target_version) = opened_migration_draft();
    apply(&mut state, SettingsMessage::Save);
    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("migration is pending"));

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::PluginVersion {
            plugin: owner,
            version: target_version,
        }),
    );
    let mut target_config = TypedMap::new();
    target_config.insert(
        Id::parse("region").unwrap_or_else(|error| panic!("field fixture: {error}")),
        TypedValue::String("us-east".to_owned()),
    );
    apply(
        &mut state,
        SettingsMessage::MigrationCompleted {
            draft_token: pending.draft_token.get(),
            target_config,
            notes: Vec::new(),
        },
    );

    assert!(matches!(
        state.settings_state.plugin_config_migration,
        PluginConfigMigrationState::Idle
    ));
    assert!(!matches!(draft_status(&state), DraftStatus::Saving { .. }));
}

#[test]
fn invalid_migration_target_fails_without_scheduling_a_write() {
    let (mut state, _, _) = opened_migration_draft();
    apply(&mut state, SettingsMessage::Save);
    let pending = state
        .pending_plugin_config_migration()
        .unwrap_or_else(|| panic!("migration is pending"));

    apply(
        &mut state,
        SettingsMessage::MigrationCompleted {
            draft_token: pending.draft_token.get(),
            target_config: TypedMap::new(),
            notes: Vec::new(),
        },
    );

    assert!(matches!(
        state.settings_state.plugin_config_migration,
        PluginConfigMigrationState::Failed { .. }
    ));
    assert!(!matches!(draft_status(&state), DraftStatus::Saving { .. }));
}

// ── CW07-01: the draft is bound to exact bytes, hash and revision ──────────

#[test]
fn opening_binds_the_draft_to_the_exact_bytes_hash_and_revision() {
    let mut state = AppState::default();
    let mut source = source(Some(SCHEMA_2));
    source.revision = 12;

    apply(&mut state, SettingsMessage::Open(Box::new(source)));

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("opening binds a draft");
    };
    assert_eq!(draft.base().document().original_bytes(), SCHEMA_2);
    assert_eq!(draft.base_hash(), Some(Sha256::digest(SCHEMA_2)));
    assert_eq!(draft.base_revision(), 12);
    assert_eq!(state.screen(), ScreenId::Settings);
}

#[test]
fn a_fresh_draft_is_clean_with_no_edits_no_preview_and_no_diagnostics() {
    let state = opened(Some(SCHEMA_2));

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("opening binds a draft");
    };
    assert_eq!(draft.status(), &DraftStatus::Clean);
    assert_eq!(draft.edited_paths().count(), 0);
    assert!(draft.preview().is_none());
    assert!(draft.validation().is_empty());
    assert!(!draft.is_dirty());
}

#[test]
fn an_absent_settings_file_still_binds_a_draft_a_save_would_create() {
    let state = opened(None);

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("an absent settings file is a normal base");
    };
    assert_eq!(draft.base_hash(), None);
    assert!(state.settings_state.blocked.is_empty());
}

#[test]
fn a_document_that_cannot_be_edited_is_reported_rather_than_half_bound() {
    let state = opened(Some(b"settings_schema = 2\n[appearance]\ntheme = 42\n"));

    assert!(state.settings_state.draft.is_none());
    let Some(first) = state.settings_state.blocked.first() else {
        panic!("a blocked document reports why");
    };
    assert_eq!(first.code, CfgCode::E003);
}

// ── CW07-02: an unsaved draft changes no active registry ──────────────────

#[test]
fn an_unsaved_edit_leaves_the_published_settings_and_screen_registry_alone() {
    let mut state = opened(Some(SCHEMA_2));
    let before_screen = state.screen();

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::InitialScreen(
            crate::domain::Id::parse("core.errors")
                .unwrap_or_else(|error| panic!("screen id fixture: {error}")),
        )),
    );

    assert_eq!(
        state.screen(),
        before_screen,
        "a structural draft moves no session"
    );
    assert_eq!(
        crate::workbench::screen_registry()
            .map(|registry| registry.screens().len())
            .unwrap_or_default(),
        ScreenId::ALL.len(),
        "the compiled registry is unchanged while the draft is unsaved"
    );
    assert_eq!(draft_status(&state), DraftStatus::Dirty);
}

#[test]
fn an_edit_records_exactly_the_path_it_wrote() {
    let mut state = opened(Some(SCHEMA_2));

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::OverrideAgentTheme(true)),
    );

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("opening binds a draft");
    };
    assert_eq!(
        draft.edited_paths().cloned().collect::<Vec<_>>(),
        vec![SyntaxPath::OverrideAgentTheme]
    );
    assert_eq!(
        draft.published().appearance.override_agent_theme,
        Some(true)
    );
}

#[test]
fn editing_a_value_back_to_where_it_started_leaves_nothing_unsaved() {
    let mut state = opened(Some(SCHEMA_2));

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    assert!(state.settings_state.is_dirty());

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("green-screen"))),
    );

    assert!(
        !state.settings_state.is_dirty(),
        "a draft that would write the same bytes holds nothing unsaved"
    );
    assert_eq!(draft_status(&state), DraftStatus::Clean);
}

#[test]
fn resetting_removes_the_source_assignment() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Reset(SyntaxPath::Theme));

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("opening binds a draft");
    };
    assert_eq!(draft.published().appearance.theme, None);
    assert!(draft.is_dirty());
}

// ── CW07-03: the theme preview is reversible ─────────────────────────────

#[test]
fn a_theme_edit_shows_a_preview_that_remembers_the_theme_it_replaced() {
    let mut state = opened(Some(SCHEMA_2));

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    let Some(preview) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(|draft| draft.preview().cloned())
    else {
        panic!("a theme edit puts a preview in flight");
    };
    assert_eq!(preview.preview_theme(), &theme("dracula"));
    assert_eq!(preview.prior_theme(), &theme("green-screen"));
    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("dracula"))
    );
}

#[test]
fn an_uninstalled_theme_is_refused_and_leaves_the_draft_alone() {
    let mut state = opened(Some(SCHEMA_2));

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("missing-theme"))),
    );

    assert!(!state.settings_state.is_dirty());
    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen"))
    );
}

#[test]
fn discarding_restores_the_exact_theme_the_screen_opened_on() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Discard);

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen"))
    );
    assert!(
        state
            .settings_state
            .draft
            .as_ref()
            .is_some_and(|draft| draft.preview().is_none())
    );
    assert!(!state.settings_state.is_dirty());
}

#[test]
fn a_successful_save_adopts_the_preview_as_the_theme_the_screen_opened_on() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);

    complete_written(&mut state);

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("dracula"))
    );
    assert!(
        state
            .settings_state
            .draft
            .as_ref()
            .is_some_and(|draft| draft.preview().is_none())
    );
}

#[test]
fn a_failed_save_restores_the_theme_the_screen_opened_on() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);

    complete(
        &mut state,
        SettingsSaveOutcome::Failed {
            revision,
            diagnostic: Box::new(write_failure()),
        },
    );

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen")),
        "a save that did not happen leaves no theme behind"
    );
    assert!(state.settings_state.is_dirty(), "the draft still holds it");
}

#[test]
fn a_conflict_restores_the_theme_the_screen_opened_on() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);

    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            revision,
            disk_hash: None,
        },
    );

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen"))
    );
}

#[test]
fn leaving_with_an_unsaved_preview_leaves_the_prior_theme_to_restore() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Discard);
    apply(&mut state, SettingsMessage::Back);

    assert!(!state.settings_state.active);
    assert_eq!(
        state.settings_state.desired_theme(),
        None,
        "a draft that never previewed leaves nothing to undo"
    );
}

#[test]
fn a_reload_keeps_the_theme_the_screen_opened_on() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    // The boundary re-reads while the session is wearing the preview, so the
    // source it reports as active is the preview, not where the user started.
    let mut reloaded = source(Some(SCHEMA_2));
    reloaded.active_theme = theme("dracula");

    apply(&mut state, SettingsMessage::Reloaded(Box::new(reloaded)));

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen")),
        "the theme the screen opened on survives a reload"
    );
}

#[test]
fn resetting_the_theme_previews_the_compiled_default_rather_than_what_was_showing() {
    // The session is wearing what the document says, as it is after startup.
    let mut opened_on = source(Some(
        br"settings_schema = 2
[appearance]
theme = 'dracula'
",
    ));
    opened_on.active_theme = theme("dracula");
    let mut state = AppState::default();
    apply(&mut state, SettingsMessage::Open(Box::new(opened_on)));
    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("dracula"))
    );

    apply(&mut state, SettingsMessage::Reset(SyntaxPath::Theme));

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen")),
        "removing the assignment means the compiled default, and the session shows it"
    );
}

#[test]
fn a_completion_for_a_superseded_conflict_leaves_the_newest_save_alone() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let superseded_revision = pending_revision(&state);
    complete(
        &mut state,
        SettingsSaveOutcome::Failed {
            revision: superseded_revision,
            diagnostic: Box::new(write_failure()),
        },
    );
    apply(&mut state, SettingsMessage::Save);
    let newest = pending_revision(&state);
    assert!(newest > superseded_revision);

    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            revision: superseded_revision,
            disk_hash: None,
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Saving { revision: newest },
        "a conflict answering superseded work is not the newest save's answer"
    );
}

// ── CW07-05: validation blocks a save without touching the draft ─────────

#[test]
fn a_reload_onto_an_invalid_document_keeps_the_draft_and_blocks_the_save() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(
        &mut state,
        SettingsMessage::Reloaded(Box::new(source(Some(
            b"settings_schema = 2\n[appearance]\ntheme = 42\n",
        )))),
    );

    assert!(state.settings_state.draft.is_none());
    let diagnostics = settings_view::diagnostics(&state.settings_state);
    let Some(first) = diagnostics.first() else {
        panic!("an invalid document reports why");
    };
    assert_eq!(first.severity, Severity::Error);
    assert_eq!(
        settings_view::first_error_row(&state.settings_state),
        Some(0)
    );
}

#[test]
fn a_blocked_draft_cannot_schedule_a_save() {
    let mut state = opened(Some(b"settings_schema = 2\n[appearance]\ntheme = 42\n"));

    apply(&mut state, SettingsMessage::Save);

    assert!(state.settings_state.draft.is_none());
    assert_eq!(draft_status(&state), DraftStatus::Clean);
}

#[test]
fn a_clean_draft_has_nothing_to_save() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Save);

    assert_eq!(draft_status(&state), DraftStatus::Clean);
    assert!(state.settings_state.notice.is_some());
}

// ── Save scheduling and CW07-09 stale completions ────────────────────────

include!("settings_save_tests.rs");
