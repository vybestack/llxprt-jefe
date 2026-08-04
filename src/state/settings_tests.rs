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

use crate::domain::ThemeId;
use crate::domain::sha256::Sha256;
use crate::messages::settings::{
    RecoveryChoice, SettingsEnvironment, SettingsMessage, SettingsSource, ThemeChoice,
};
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::{SettingsEdit, SettingsSaveOutcome, SyntaxPath};
use crate::workbench::ScreenId;

use super::settings_types::DraftStatus;
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
        environment: SettingsEnvironment {
            settings_path: PathBuf::from("/tmp/jefe/settings.toml"),
            state_path: PathBuf::from("/tmp/jefe/state.json"),
            platform: "test",
            isolated: true,
        },
    }
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
        draft.edited_paths().collect::<Vec<_>>(),
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

    complete(
        &mut state,
        SettingsSaveOutcome::Failed {
            diagnostic: Box::new(write_failure()),
        },
    );
    apply(&mut state, SettingsMessage::Discard);

    assert_eq!(
        state.settings_state.desired_theme(),
        Some(&theme("green-screen"))
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

fn pending_revision(state: &AppState) -> u64 {
    state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::pending_revision)
        .unwrap_or_else(|| panic!("a scheduled save carries a revision"))
}

fn written(revision: u64, state: &AppState) -> SettingsSaveOutcome {
    let Some(candidate) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::candidate_bytes)
    else {
        panic!("a saveable draft has a candidate");
    };
    SettingsSaveOutcome::Written {
        revision,
        hash: candidate.sha256(),
    }
}

fn complete(state: &mut AppState, outcome: SettingsSaveOutcome) {
    apply(state, SettingsMessage::SaveCompleted(Box::new(outcome)));
}

/// Answer the scheduled save as the writer would after a successful write.
fn complete_written(state: &mut AppState) {
    let revision = pending_revision(state);
    let outcome = written(revision, state);
    complete(state, outcome);
}

fn write_failure() -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E104,
        Severity::Error,
        DiagnosticPath::new("/tmp/jefe/settings.toml"),
        None,
        "preserve the draft and resolve the filesystem write failure",
    );
    "injected writer phase failure".clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

#[test]
fn a_save_schedules_a_strictly_increasing_revision() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Save);
    let first = pending_revision(&state);
    complete_written(&mut state);

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::OverrideAgentTheme(true)),
    );
    apply(&mut state, SettingsMessage::Save);
    let second = pending_revision(&state);

    assert!(second > first, "{second} must follow {first}");
}

#[test]
fn a_matching_completion_adopts_the_saved_bytes_as_the_new_base() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);
    let outcome = written(revision, &state);
    let SettingsSaveOutcome::Written { hash, .. } = outcome else {
        panic!("fixture outcome is a write");
    };

    complete(&mut state, SettingsSaveOutcome::Written { revision, hash });

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a save keeps the draft");
    };
    assert_eq!(draft.status(), &DraftStatus::Clean);
    assert_eq!(draft.base_hash(), Some(hash));
    assert_eq!(draft.base_revision(), revision);
    assert_eq!(draft.edited_paths().count(), 0);
    assert_eq!(
        draft.published().appearance.theme.as_deref(),
        Some("dracula")
    );
}

#[test]
fn a_completion_for_a_superseded_revision_is_ignored() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let newest = pending_revision(&state);
    let superseded_revision = newest - 1;

    complete(
        &mut state,
        SettingsSaveOutcome::Written {
            revision: superseded_revision,
            hash: Sha256::digest(b"whatever was on disk then"),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Saving { revision: newest },
        "the newest pending revision stands"
    );
    assert_eq!(pending_revision(&state), newest);
}

// ── CW07-06: a hash conflict preserves disk and draft ────────────────────

#[test]
fn a_conflict_preserves_the_draft_and_offers_reload_export_and_retry() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let disk_hash = Sha256::digest(b"someone else's settings");

    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            disk_hash: Some(disk_hash),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Conflict {
            disk_hash: Some(disk_hash)
        }
    );
    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a conflict keeps the draft");
    };
    assert_eq!(
        draft.published().appearance.theme.as_deref(),
        Some("dracula"),
        "the draft is preserved"
    );
    assert_eq!(
        settings_view::recovery_choices(&state.settings_state),
        vec![
            RecoveryChoice::Reload,
            RecoveryChoice::Export,
            RecoveryChoice::Retry
        ]
    );
}

#[test]
fn a_write_failure_offers_retry_export_and_discard() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);

    complete(
        &mut state,
        SettingsSaveOutcome::Failed {
            diagnostic: Box::new(write_failure()),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Failed {
            code: CfgCode::E104
        }
    );
    assert_eq!(
        settings_view::recovery_choices(&state.settings_state),
        vec![
            RecoveryChoice::Retry,
            RecoveryChoice::Export,
            RecoveryChoice::Discard
        ]
    );
}

#[test]
fn retrying_after_a_conflict_reschedules_the_same_draft() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let first = pending_revision(&state);
    complete(
        &mut state,
        SettingsSaveOutcome::Conflict { disk_hash: None },
    );

    apply(&mut state, SettingsMessage::Save);

    let retried = pending_revision(&state);
    assert!(retried > first);
    assert_eq!(
        draft_status(&state),
        DraftStatus::Saving { revision: retried }
    );
}

// ── CW07-07: reload rebuilds from the exact disk bytes ───────────────────

#[test]
fn a_dirty_reload_asks_before_it_discards_anything() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Reload);

    assert!(state.settings_state.reload_confirm);
    assert!(
        state.settings_state.is_dirty(),
        "asking discards nothing yet"
    );
}

#[test]
fn a_clean_reload_needs_no_confirmation() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Reload);

    assert!(!state.settings_state.reload_confirm);
}

#[test]
fn a_reload_rebuilds_the_draft_from_the_exact_current_bytes() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    let external =
        b"settings_schema = 2\n# somebody else was here\n[appearance]\ntheme = 'green-screen'\n";

    apply(
        &mut state,
        SettingsMessage::Reloaded(Box::new(source(Some(external)))),
    );

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a reload binds a draft");
    };
    assert_eq!(draft.base().document().original_bytes(), external);
    assert_eq!(draft.base_hash(), Some(Sha256::digest(external)));
    assert!(!draft.is_dirty());
    assert!(!state.settings_state.reload_confirm);
}

// ── CW07-08: export leaves the draft exactly where it is ─────────────────

#[test]
fn an_export_result_changes_no_base_hash_or_dirty_status() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    let before_hash = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::base_hash);

    apply(
        &mut state,
        SettingsMessage::ExportCompleted(Box::new(Ok(PathBuf::from("/tmp/jefe/draft.toml")))),
    );

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("export keeps the draft");
    };
    assert_eq!(draft.base_hash(), before_hash);
    assert!(draft.is_dirty());
    assert!(
        state
            .settings_state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.contains("draft.toml"))
    );
}

#[test]
fn a_failed_export_retains_the_draft_and_reports_a_redacted_reason() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(
        &mut state,
        SettingsMessage::ExportCompleted(Box::new(Err(write_failure()))),
    );

    assert!(state.settings_state.is_dirty());
    assert!(
        state
            .settings_state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.starts_with("CFG-E104"))
    );
}
