//! Behavioral tests for lossless schema-2 settings candidate editing.
//!
//! @requirement CW07-04
//! @requirement CW07-08
//! @requirement CW07-10

use std::path::Path;

use crate::domain::sha256::Sha256;
use crate::domain::{Id, OwnerCatalog, ThemeId};

use super::diagnostic::CfgCode;
use super::migration::migrate_settings;
use super::settings_edit::{
    EDITED_PATH_LIMIT, ExportPath, SettingsCandidate, SettingsEdit, SettingsSaveOutcome,
    SyntaxPath, export_candidate, load_settings_base,
};
use super::writer::{ExpectedHash, Freshness};
use super::{FilePersistenceManager, PersistencePaths};

fn catalog() -> OwnerCatalog {
    crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"))
}

fn theme(slug: &str) -> ThemeId {
    ThemeId::parse(slug).unwrap_or_else(|error| panic!("theme fixture: {error}"))
}

fn screen(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("screen id fixture: {error}"))
}

/// Build a candidate over `source`, applying `edits` in order.
fn candidate(source: &[u8], edits: &[SettingsEdit]) -> SettingsCandidate {
    let catalog = catalog();
    let migration = migrate_settings(source, &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture must load: {diagnostics:?}"));
    SettingsCandidate::from_edits(
        &migration,
        &catalog,
        edits,
        ExpectedHash::Present(Sha256::digest(source)),
    )
    .unwrap_or_else(|diagnostics| panic!("valid candidate must compose: {diagnostics:?}"))
}

/// The sorted diagnostics that stop `source` from becoming an editable base.
fn blocked(source: &[u8]) -> Vec<super::diagnostic::Diagnostic> {
    match load_settings_base(Some(source), &catalog()) {
        Ok(_) => panic!("an uneditable document must be refused"),
        Err(diagnostics) => diagnostics,
    }
}

// ── CW07-04: patch only the edited syntax paths ────────────────────────────

#[test]
fn theme_edit_replaces_only_its_own_value_and_keeps_every_other_byte() {
    let source = br#"# top comment
settings_schema = 2

[appearance]
theme = 'green-screen' # trailing note
override_agent_theme = true

[keymap.global]
"core.emergency-exit" = ["Ctrl+Q"]

[extensions.future]
opaque = { bytes = "retained" }
"#;

    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);

    let expected = br#"# top comment
settings_schema = 2

[appearance]
theme = "dracula" # trailing note
override_agent_theme = true

[keymap.global]
"core.emergency-exit" = ["Ctrl+Q"]

[extensions.future]
opaque = { bytes = "retained" }
"#;
    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        String::from_utf8_lossy(expected)
    );
    assert!(
        candidate
            .published()
            .dormant
            .iter()
            .any(|entry| entry.path == ["extensions"]),
        "the dormant extensions subtree stays dormant"
    );
}

#[test]
fn override_toggle_replaces_only_its_own_value() {
    let source = br"settings_schema = 2
[appearance]
theme = 'green-screen'
override_agent_theme = false
";

    let candidate = candidate(source, &[SettingsEdit::OverrideAgentTheme(true)]);

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\n[appearance]\ntheme = 'green-screen'\noverride_agent_theme = true\n"
    );
}

#[test]
fn a_missing_assignment_is_inserted_after_the_last_statement_of_its_table() {
    let source = br#"settings_schema = 2
[appearance]
theme = 'green-screen'
[extensions.future]
opaque = "retained"
"#;

    let candidate = candidate(source, &[SettingsEdit::OverrideAgentTheme(true)]);

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\n[appearance]\ntheme = 'green-screen'\noverride_agent_theme = true\n[extensions.future]\nopaque = \"retained\"\n"
    );
}

#[test]
fn a_missing_table_is_appended_without_disturbing_existing_bytes() {
    let source = br#"settings_schema = 2
[keymap.global]
"core.emergency-exit" = ["Ctrl+Q"]
"#;

    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\n[keymap.global]\n\"core.emergency-exit\" = [\"Ctrl+Q\"]\n[appearance]\ntheme = \"dracula\"\n"
    );
}

#[test]
fn a_dotted_root_assignment_is_patched_in_place() {
    let source = b"settings_schema = 2\nappearance.theme = 'green-screen'\n";

    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\nappearance.theme = \"dracula\"\n"
    );
}

#[test]
fn reset_removes_only_the_selected_statement() {
    let source = br"settings_schema = 2
[appearance]
theme = 'green-screen'
override_agent_theme = true
";

    let candidate = candidate(source, &[SettingsEdit::Reset(SyntaxPath::Theme)]);

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\n[appearance]\noverride_agent_theme = true\n"
    );
    assert_eq!(candidate.published().appearance.theme, None);
}

#[test]
fn resetting_an_absent_assignment_changes_nothing() {
    let source = b"settings_schema = 2\n";

    let candidate = candidate(source, &[SettingsEdit::Reset(SyntaxPath::Theme)]);

    assert_eq!(candidate.bytes(), source);
}

#[test]
fn several_edits_compose_into_one_candidate() {
    let source = b"settings_schema = 2\n";

    let candidate = candidate(
        source,
        &[
            SettingsEdit::Theme(theme("dracula")),
            SettingsEdit::OverrideAgentTheme(true),
            SettingsEdit::InitialScreen(screen("core.errors")),
        ],
    );

    assert_eq!(
        String::from_utf8_lossy(candidate.bytes()),
        "settings_schema = 2\n[appearance]\ntheme = \"dracula\"\noverride_agent_theme = true\n[workbench]\ninitial_screen = \"core.errors\"\n"
    );
    assert_eq!(
        candidate.published().appearance.theme.as_deref(),
        Some("dracula")
    );
    assert_eq!(
        candidate.published().workbench.initial_screen,
        Some(screen("core.errors"))
    );
}

// ── CW07-10: schema-1 view saves as an explicit schema 2 ───────────────────

#[test]
fn a_schema_1_view_saves_as_schema_2_and_keeps_dormant_syntax() {
    let source = br#"schema_version = 1
theme = "dracula"
override_agent_theme = true
future_root = "kept"

[legacy_table]
value = 1
"#;

    let candidate = candidate(source, &[SettingsEdit::Theme(theme("atom-one-dark"))]);
    let rendered = String::from_utf8_lossy(candidate.bytes()).into_owned();

    assert!(
        rendered.starts_with("settings_schema = 2\n"),
        "an explicit schema-2 document is written: {rendered}"
    );
    assert!(
        !rendered.contains("schema_version = 1"),
        "schema 1 is not retained: {rendered}"
    );
    assert!(
        rendered.contains("theme = \"atom-one-dark\""),
        "the edit is applied: {rendered}"
    );
    assert!(
        rendered.contains("future_root = \"kept\""),
        "the unknown root assignment stays byte-preserved: {rendered}"
    );
    assert!(
        rendered.contains("[extensions.schema1.legacy_table]"),
        "the unknown table stays dormant: {rendered}"
    );
}

#[test]
fn reading_a_schema_1_document_never_rewrites_it() {
    let source = b"schema_version = 1\ntheme = \"dracula\"\n";
    let catalog = catalog();

    let migration = migrate_settings(source, &catalog)
        .unwrap_or_else(|diagnostics| panic!("schema-1 fixture must load: {diagnostics:?}"));

    assert!(migration.was_migrated());
    assert_eq!(migration.document().original_bytes(), source);
}

// ── Complete-candidate validation ──────────────────────────────────────────

#[test]
fn a_hand_authored_type_error_blocks_the_base() {
    let diagnostics = blocked(b"settings_schema = 2\n[appearance]\ntheme = 42\n");

    let Some(first) = diagnostics.first() else {
        panic!("a refusal must carry a diagnostic");
    };
    assert_eq!(first.code, CfgCode::E003);
    assert_eq!(first.path.as_str(), "/appearance/theme");
}

#[test]
fn an_unowned_root_blocks_the_base() {
    let diagnostics = blocked(b"settings_schema = 2\n[nonsense]\nvalue = 1\n");

    let Some(first) = diagnostics.first() else {
        panic!("a refusal must carry a diagnostic");
    };
    assert_eq!(first.code, CfgCode::E005);
}

#[test]
fn a_wrong_boolean_type_blocks_the_base() {
    let diagnostics =
        blocked(b"settings_schema = 2\n[appearance]\noverride_agent_theme = \"yes\"\n");

    let Some(first) = diagnostics.first() else {
        panic!("a refusal must carry a diagnostic");
    };
    assert_eq!(first.code, CfgCode::E003);
    assert_eq!(first.path.as_str(), "/appearance/override_agent_theme");
}

#[test]
fn an_absent_settings_file_binds_to_the_empty_schema_2_document() {
    let Ok(migration) = load_settings_base(None, &catalog()) else {
        panic!("an absent settings file is a normal base");
    };

    assert!(!migration.was_migrated());
    assert_eq!(
        migration.document().original_bytes(),
        b"settings_schema = 2\n"
    );
    assert_eq!(migration.published().appearance.theme, None);
}

#[test]
fn a_secret_reference_survives_export_as_a_reference() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = br#"settings_schema = 2
[agents."core.codex"]
repository_defaults = { token = { secret_ref = "core.codex-token" } }
"#;
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);
    let Ok(relative) = ExportPath::parse("settings-draft.toml") else {
        panic!("contained export path fixture");
    };

    let Ok(written) = export_candidate(&candidate, dir.path(), &relative, &catalog()) else {
        panic!("a contained export must succeed");
    };

    let Ok(exported) = std::fs::read(&written) else {
        panic!("the export must be readable");
    };
    let rendered = String::from_utf8_lossy(&exported);
    assert!(
        rendered.contains("secret_ref"),
        "a secret stays a reference: {rendered}"
    );
    assert!(
        rendered.contains("core.codex-token"),
        "the reference identity is retained: {rendered}"
    );
}

// ── Candidate identity and bounds ──────────────────────────────────────────

#[test]
fn the_candidate_hash_is_the_digest_of_its_own_bytes() {
    let candidate = candidate(
        b"settings_schema = 2\n",
        &[SettingsEdit::Theme(theme("dracula"))],
    );

    assert_eq!(candidate.sha256(), Sha256::digest(candidate.bytes()));
}

#[test]
fn every_editable_path_fits_within_the_edited_path_limit() {
    assert!(
        SyntaxPath::ALL.len() <= EDITED_PATH_LIMIT,
        "the closed editable path set cannot exceed the documented bound"
    );
}

#[test]
fn only_the_start_screen_requires_a_restart() {
    assert!(SyntaxPath::InitialScreen.structural());
    assert!(!SyntaxPath::Theme.structural());
    assert!(!SyntaxPath::OverrideAgentTheme.structural());

    let cosmetic = candidate(
        b"settings_schema = 2\n",
        &[SettingsEdit::Theme(theme("dracula"))],
    );
    assert!(!cosmetic.structural());

    let structural = candidate(
        b"settings_schema = 2\n",
        &[SettingsEdit::InitialScreen(screen("core.errors"))],
    );
    assert!(structural.structural());
}

#[test]
fn every_editable_path_names_a_distinct_owned_leaf() {
    let mut paths: Vec<&[&str]> = SyntaxPath::ALL.iter().map(|path| path.segments()).collect();
    paths.sort_unstable();
    let count = paths.len();
    paths.dedup();
    assert_eq!(paths.len(), count, "editable paths must be distinct");

    for path in SyntaxPath::ALL {
        assert!(
            matches!(path.segments().first(), Some(&"appearance" | &"workbench")),
            "every editable leaf lives under a host-owned root"
        );
    }
}

// ── Save boundary ──────────────────────────────────────────────────────────

fn manager(dir: &Path) -> FilePersistenceManager {
    FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: dir.join("settings.toml"),
        state_path: dir.join("state.json"),
    })
}

fn current(_revision: u64) -> Freshness {
    Freshness::Current
}

fn superseded(_revision: u64) -> Freshness {
    Freshness::Stale
}

#[test]
fn a_matching_hash_save_writes_the_candidate_and_reports_its_hash() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = b"settings_schema = 2\n[appearance]\ntheme = 'green-screen'\n";
    let path = dir.path().join("settings.toml");
    assert!(
        std::fs::write(&path, source).is_ok(),
        "settings fixture must be writable"
    );
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);

    let outcome = manager(dir.path()).save_settings_candidate_revisioned(&candidate, 7, &current);

    let SettingsSaveOutcome::Written { hash, revision } = outcome else {
        panic!("a matching save is authoritative, not {outcome:?}");
    };
    assert_eq!(revision, 7);
    assert_eq!(hash, candidate.sha256());
    let Ok(written) = std::fs::read(&path) else {
        panic!("the target must be readable");
    };
    assert_eq!(written, candidate.bytes());
}

#[test]
fn a_superseded_revision_leaves_the_target_untouched() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = b"settings_schema = 2\n";
    let path = dir.path().join("settings.toml");
    assert!(
        std::fs::write(&path, source).is_ok(),
        "settings fixture must be writable"
    );
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);

    let outcome =
        manager(dir.path()).save_settings_candidate_revisioned(&candidate, 3, &superseded);

    assert!(matches!(
        outcome,
        SettingsSaveOutcome::Superseded { revision: 3 }
    ));
    let Ok(written) = std::fs::read(&path) else {
        panic!("the target must be readable");
    };
    assert_eq!(written, source);
}

#[test]
fn an_external_edit_is_reported_as_a_conflict_carrying_the_disk_hash() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = b"settings_schema = 2\n";
    let path = dir.path().join("settings.toml");
    assert!(
        std::fs::write(&path, source).is_ok(),
        "settings fixture must be writable"
    );
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);
    let external = b"settings_schema = 2\n[appearance]\ntheme = 'atom-one-dark'\n";
    assert!(
        std::fs::write(&path, external).is_ok(),
        "external edit fixture must be writable"
    );

    let outcome = manager(dir.path()).save_settings_candidate_revisioned(&candidate, 4, &current);

    let SettingsSaveOutcome::Conflict { disk_hash } = outcome else {
        panic!("a changed target conflicts, not {outcome:?}");
    };
    assert_eq!(disk_hash, Some(Sha256::digest(external)));
    let Ok(written) = std::fs::read(&path) else {
        panic!("the target must be readable");
    };
    assert_eq!(written, external, "the disk bytes are preserved");
}

// ── Export ─────────────────────────────────────────────────────────────────

#[test]
fn export_paths_must_be_relative_and_contained() {
    assert!(ExportPath::parse("settings-draft.toml").is_ok());
    assert!(ExportPath::parse("drafts/settings.toml").is_ok());
    assert!(ExportPath::parse("").is_err());
    assert!(ExportPath::parse("../escape.toml").is_err());
    assert!(ExportPath::parse("drafts/../../escape.toml").is_err());
    assert!(ExportPath::parse("/absolute.toml").is_err());
    assert!(ExportPath::parse("./relative.toml").is_err());
}

#[test]
fn export_writes_the_draft_without_changing_the_candidate() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = b"settings_schema = 2\n# kept\n";
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);
    let before = candidate.bytes().to_vec();
    let Ok(relative) = ExportPath::parse("drafts/settings-draft.toml") else {
        panic!("contained export path fixture");
    };

    let Ok(written) = export_candidate(&candidate, dir.path(), &relative, &catalog()) else {
        panic!("a contained export must succeed");
    };

    assert_eq!(written, dir.path().join("drafts/settings-draft.toml"));
    let Ok(exported) = std::fs::read(&written) else {
        panic!("the export must be readable");
    };
    assert!(String::from_utf8_lossy(&exported).contains("theme = \"dracula\""));
    assert_eq!(
        candidate.bytes(),
        before.as_slice(),
        "export leaves the candidate untouched"
    );
    assert_mode_user_only(&written);
}

#[test]
fn export_refuses_an_existing_target_and_retains_it() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp dir fixture");
    };
    let source = b"settings_schema = 2\n";
    let candidate = candidate(source, &[SettingsEdit::Theme(theme("dracula"))]);
    let Ok(relative) = ExportPath::parse("settings-draft.toml") else {
        panic!("contained export path fixture");
    };
    let target = dir.path().join("settings-draft.toml");
    assert!(
        std::fs::write(&target, b"existing").is_ok(),
        "existing target fixture must be writable"
    );

    let Err(diagnostic) = export_candidate(&candidate, dir.path(), &relative, &catalog()) else {
        panic!("an occupied export target must be refused");
    };

    assert_eq!(diagnostic.code, CfgCode::E104);
    let Ok(retained) = std::fs::read(&target) else {
        panic!("the target must be readable");
    };
    assert_eq!(retained, b"existing", "the existing file is retained");
}

#[cfg(unix)]
fn assert_mode_user_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(path) else {
        panic!("the export must have metadata");
    };
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}

#[cfg(not(unix))]
fn assert_mode_user_only(_path: &Path) {}
