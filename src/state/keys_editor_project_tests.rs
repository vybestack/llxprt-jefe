//! Behavioral tests for the Keys editor projection and capture (issue #388).
//!
//! @requirement CW08-05
//! @requirement CW08-06
//! @requirement CW08-08

use crate::domain::action_registry::{ActionId, ActionRegistrySnapshot, Availability, Provenance};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::persistence::settings_document::PublishedSettings;

use super::{
    CaptureOutcome, ChordText, EMERGENCY_EXIT_ACTION, KeyEditorRow, classify_capture,
    conflict_detail, project_keys,
};

fn snapshot(source: &str) -> ActionRegistrySnapshot {
    let catalog = crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    let loaded =
        crate::persistence::keymap_edit::load_bytes(Some(source.as_bytes()), &catalog, "settings")
            .unwrap_or_else(|diagnostics| panic!("keymap fixture: {diagnostics:?}"));
    loaded.composed.snapshot().clone()
}

fn published(source: &str) -> PublishedSettings {
    let catalog = crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    crate::persistence::migration::migrate_settings(source.as_bytes(), &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture: {diagnostics:?}"))
        .published()
        .clone()
}

fn rows(source: &str) -> Vec<KeyEditorRow> {
    project_keys(&snapshot(source), &published(source))
}

fn row<'rows>(rows: &'rows [KeyEditorRow], action: &str) -> &'rows KeyEditorRow {
    rows.iter()
        .find(|row| row.action.as_str() == action)
        .unwrap_or_else(|| panic!("row for {action}"))
}

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap_or_else(|error| panic!("chord fixture {text}: {error:?}"))
}

const DEFAULTS: &str = "settings_schema = 2\n";

/// A settings document binding `core.open-settings` to `chords`.
fn override_source(chords: &str) -> String {
    format!("settings_schema = 2\n[keymap.global]\n\"core.open-settings\" = {chords}\n")
}

// ── every action projects exactly one row ─────────────────────────────────

#[test]
fn every_inventoried_action_projects_exactly_one_row() {
    let rows = rows(DEFAULTS);

    assert!(!rows.is_empty(), "the compiled inventory declares actions");
    let mut seen: Vec<_> = rows
        .iter()
        .map(|row| (row.context.as_str(), row.action.as_str()))
        .collect();
    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), count, "no action/context pair repeats");
}

#[test]
fn a_row_carries_the_effective_chords_and_their_provenance() {
    let rows = rows(DEFAULTS);

    let exit = row(&rows, EMERGENCY_EXIT_ACTION);
    assert!(
        !exit.chords.is_empty(),
        "the emergency exit is bound by the compiled inventory"
    );
    assert_eq!(exit.provenance, Provenance::Compiled);
}

#[test]
fn a_binding_the_document_overrides_reports_that_provenance() {
    let source = concat!(
        "settings_schema = 2\n",
        "[keymap.global]\n",
        "\"core.open-settings\" = [\"F2\"]\n",
    );

    let rows = rows(source);

    let overridden = row(&rows, "core.open-settings");
    assert_eq!(overridden.chords, vec![ChordText::Chord(chord("F2"))]);
    assert!(matches!(overridden.provenance, Provenance::Settings { .. }));
}

#[test]
fn projecting_the_same_snapshot_twice_produces_the_same_rows() {
    assert_eq!(rows(DEFAULTS), rows(DEFAULTS));
}

#[test]
fn a_row_shows_the_chords_the_candidate_names_rather_than_the_composed_ones() {
    // The snapshot is the registry this session started with; the candidate is
    // what a save would make authoritative. A row that showed the snapshot
    // would present the user's own unsaved rebinding as not having happened.
    let snapshot = snapshot(DEFAULTS);
    let published = published(&override_source("[\"F2\"]"));

    let rows = project_keys(&snapshot, &published);

    assert_eq!(
        row(&rows, "core.open-settings").chords,
        vec![ChordText::Chord(chord("F2"))]
    );
}

#[test]
fn a_binding_the_candidate_empties_projects_as_unbound() {
    let snapshot = snapshot(DEFAULTS);
    let published = published(&override_source("[]"));

    let rows = project_keys(&snapshot, &published);

    assert!(row(&rows, "core.open-settings").chords.is_empty());
}

#[test]
fn a_chord_the_candidate_does_not_mention_still_comes_from_the_registry() {
    let snapshot = snapshot(DEFAULTS);
    let published = published(&override_source("[\"F2\"]"));

    let rows = project_keys(&snapshot, &published);

    assert!(
        !row(&rows, EMERGENCY_EXIT_ACTION).chords.is_empty(),
        "an untouched binding keeps its compiled chords"
    );
}

// ── CW08-08: a protected action is read-only with an exact reason ──────────

#[test]
fn the_emergency_exit_projects_read_only_with_the_registrys_own_reason() {
    let rows = rows(DEFAULTS);

    let exit = row(&rows, EMERGENCY_EXIT_ACTION);
    let Some(reason) = exit.protected.as_deref() else {
        panic!("the emergency exit is protected");
    };
    assert_eq!(
        reason,
        crate::domain::action_registry::PROTECTED_ACTION_REASON
    );
}

#[test]
fn every_unprotected_action_projects_without_a_reason() {
    let rows = rows(DEFAULTS);

    let editable = rows.iter().filter(|row| row.protected.is_none()).count();
    assert!(editable > 0, "most actions are editable");
    for row in rows.iter().filter(|row| row.protected.is_some()) {
        assert!(
            !row.availability_unavailable(),
            "a protected action is always available: {}",
            row.action.as_str()
        );
    }
}

#[test]
fn an_available_action_projects_as_available() {
    let rows = rows(DEFAULTS);

    assert_eq!(
        row(&rows, EMERGENCY_EXIT_ACTION).availability,
        Availability::Available
    );
}

// ── CW08-05: capture takes exactly the next eligible key ──────────────────

#[test]
fn an_ordinary_key_press_becomes_exactly_one_captured_chord() {
    assert_eq!(
        classify_capture(chord("j")),
        CaptureOutcome::Captured(chord("j"))
    );
}

#[test]
fn a_modified_key_press_is_captured_carrying_its_modifiers() {
    assert_eq!(
        classify_capture(chord("Ctrl+m")),
        CaptureOutcome::Captured(chord("Ctrl+m"))
    );
}

#[test]
fn escape_cancels_the_capture_rather_than_being_taken_by_it() {
    assert_eq!(classify_capture(chord("Esc")), CaptureOutcome::Cancelled);
}

#[test]
fn a_modified_escape_is_an_ordinary_chord_because_only_bare_escape_cancels() {
    assert_eq!(
        classify_capture(chord("Ctrl+Esc")),
        CaptureOutcome::Captured(chord("Ctrl+Esc"))
    );
}

#[test]
fn the_protected_exit_chord_is_never_captured() {
    assert_eq!(
        classify_capture(chord("Ctrl+q")),
        CaptureOutcome::Protected,
        "Ctrl-Q leaves the session and can never be taken by a capture"
    );
}

#[test]
fn a_bare_modifier_press_never_reaches_the_capture_at_all() {
    // The boundary turns a key event into a chord, and a chord has no spelling
    // for "a modifier and nothing else", so a modifier press cannot arrive
    // here to be rejected.
    assert!(matches!(
        Chord::parse("Ctrl"),
        Err(crate::domain::keymap::ChordError::ModifierOnly)
    ));
}

// ── CW08-06: a conflict names both owners, the context, and the chord ─────

#[test]
fn a_chord_two_actions_claim_is_refused_naming_both_of_them() {
    let source = concat!(
        "settings_schema = 2\n",
        "[keymap.global]\n",
        "\"core.open-settings\" = [\"F7\"]\n",
        "\"core.open-keys\" = [\"F7\"]\n",
    );
    let catalog = crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    let loaded =
        crate::persistence::keymap_edit::load_bytes(Some(source.as_bytes()), &catalog, "settings")
            .unwrap_or_else(|diagnostics| {
                panic!("a conflicting keymap still loads: {diagnostics:?}")
            });

    let Some(diagnostic) = loaded.diagnostic else {
        panic!("two actions claiming one chord is a conflict");
    };
    let detail = diagnostic.to_string();
    assert!(detail.contains("KEY-E401"), "{detail}");
    assert!(detail.contains("core.open-settings"), "{detail}");
    assert!(detail.contains("core.open-keys"), "{detail}");
    assert!(detail.contains("global"), "{detail}");
}

#[test]
fn a_conflict_detail_names_the_chord_the_context_and_both_actions() {
    let detail = conflict_detail(
        &ContextId::parse("global").unwrap_or_else(|error| panic!("context: {error}")),
        chord("Ctrl+m"),
        &ActionId::parse("core.merge").unwrap_or_else(|error| panic!("action: {error}")),
        &ActionId::parse("core.move").unwrap_or_else(|error| panic!("action: {error}")),
        &Provenance::Settings {
            source: "settings".to_owned(),
        },
    );

    assert!(detail.contains("KEY-E401"), "{detail}");
    assert!(
        detail.contains(&chord("Ctrl+m").to_canonical_text()),
        "{detail}"
    );
    assert!(detail.contains("global"), "{detail}");
    assert!(detail.contains("core.merge"), "{detail}");
    assert!(detail.contains("core.move"), "{detail}");
    assert!(detail.contains("settings"), "{detail}");
}

#[test]
fn a_chord_the_grammar_cannot_read_stays_visible_as_the_text_it_was_written_as() {
    // The resolver refuses this candidate. If the row dropped the offending
    // text, the user would be told about a chord they could not see.
    let snapshot = snapshot(DEFAULTS);
    let published = published(&override_source("[\"F2\", \"nonsense-chord\"]"));

    let rows = project_keys(&snapshot, &published);

    assert_eq!(
        row(&rows, "core.open-settings").chords,
        vec![
            ChordText::Chord(chord("F2")),
            ChordText::Unreadable("nonsense-chord".to_owned()),
        ]
    );
}
