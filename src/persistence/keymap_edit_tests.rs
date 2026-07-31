//! Behavioral tests for lossless schema-2 keymap candidate editing.

use crate::domain::action_registry::{ActionId, Provenance, Resolution};
use crate::domain::input_context::{ContextId, ContextStack};
use crate::domain::keymap::Chord;

use super::keymap_edit::{KeymapCandidate, KeymapDiagnostic, load_bytes};
use super::settings_document::SettingsDocument;
use super::{FilePersistenceManager, PersistencePaths};

fn document(bytes: &[u8]) -> SettingsDocument {
    let Ok(document) = SettingsDocument::parse(bytes) else {
        panic!("settings fixture must parse");
    };
    document
}

fn context(value: &str) -> ContextId {
    let Ok(context) = ContextId::parse(value) else {
        panic!("context fixture must parse");
    };
    context
}

fn action(value: &str) -> ActionId {
    let Ok(action) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    action
}

fn chords(values: &[&str]) -> Vec<Chord> {
    values
        .iter()
        .map(|value| Chord::parse(value).unwrap_or_else(|error| panic!("chord fixture: {error}")))
        .collect()
}

fn catalog() -> crate::domain::OwnerCatalog {
    crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"))
}

fn counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[test]
fn set_is_a_whole_list_patch_and_snapshot_retains_effective_provenance() {
    let original = br#"# heading
settings_schema = 2
[appearance]
theme = 'green-screen' # retained
[keymap.dashboard]
"dashboard.navigate-up" = ["Up", "k"] # selected
[extensions.future]
opaque = { bytes = "retained" }
"#;
    let source = document(original);
    let candidate = KeymapCandidate::set(
        &source,
        &catalog(),
        &context("dashboard"),
        &action("dashboard.navigate-up"),
        &chords(&["w"]),
        "settings.toml",
    )
    .unwrap_or_else(|error| panic!("valid candidate must compose: {error}"));

    let expected = br#"# heading
settings_schema = 2
[appearance]
theme = 'green-screen' # retained
[keymap.dashboard]
"dashboard.navigate-up" = ["w"] # selected
[extensions.future]
opaque = { bytes = "retained" }
"#;
    assert_eq!(candidate.bytes(), expected);
    assert!(
        candidate
            .published()
            .dormant
            .iter()
            .any(|entry| entry.path == ["extensions"])
    );
    let stack = ContextStack::from_ordered(["dashboard", "global"], false)
        .unwrap_or_else(|error| panic!("stack fixture: {error}"));
    assert!(matches!(
        candidate.snapshot().resolve(&chords(&["w"])[0], &stack),
        Resolution::Dispatch { action, .. } if action.as_str() == "dashboard.navigate-up"
    ));
    let Some(binding) = candidate
        .snapshot()
        .effective_bindings()
        .iter()
        .find(|binding| {
            binding.context.as_str() == "dashboard"
                && binding.action.as_str() == "dashboard.navigate-up"
        })
    else {
        panic!("effective snapshot binding must be retained");
    };
    assert_eq!(binding.chords, chords(&["w"]));
    assert_eq!(
        binding.provenance,
        Provenance::Settings {
            source: "settings.toml".to_owned()
        }
    );
}

#[test]
fn unbind_and_reset_distinguish_empty_override_from_absence() {
    let original = br#"settings_schema = 2
[keymap.dashboard]
"dashboard.navigate-up" = ["Up", "k"] # remove only this statement
[extensions.future]
opaque = "retained"
"#;
    let source = document(original);
    let unbound = KeymapCandidate::unbind(
        &source,
        &catalog(),
        &context("dashboard"),
        &action("dashboard.navigate-up"),
        "settings.toml",
    )
    .unwrap_or_else(|error| panic!("unbind must compose: {error}"));
    assert!(
        std::str::from_utf8(unbound.bytes())
            .unwrap_or_else(|error| panic!("candidate UTF-8: {error}"))
            .contains("\"dashboard.navigate-up\" = [] # remove only this statement")
    );

    let reset = KeymapCandidate::patch(
        &source,
        &catalog(),
        &context("dashboard"),
        &action("dashboard.navigate-up"),
        None,
        "settings.toml",
    )
    .unwrap_or_else(|error| panic!("reset must compose: {error}"));
    assert_eq!(
        reset.bytes(),
        br#"settings_schema = 2
[keymap.dashboard]
[extensions.future]
opaque = "retained"
"#
    );
    let stack = ContextStack::from_ordered(["dashboard", "global"], false)
        .unwrap_or_else(|error| panic!("stack fixture: {error}"));
    assert!(matches!(
        reset.snapshot().resolve(&chords(&["Up"])[0], &stack),
        Resolution::Dispatch { action, .. } if action.as_str() == "dashboard.navigate-up"
    ));
}

#[test]
fn complete_nested_stacks_reject_implicit_and_protected_shadows() {
    let original = b"settings_schema = 2\n";
    let source = document(original);
    let implicit = KeymapCandidate::set(
        &source,
        &catalog(),
        &context("issues"),
        &action("issues.open-prs"),
        &chords(&["Down"]),
        "settings.toml",
    )
    .err()
    .unwrap_or_else(|| panic!("parent override must not implicitly shadow detail navigation"));
    assert!(implicit.to_string().contains("ImplicitShadow"));

    // Modal contexts inherit only protected global recovery controls, not the
    // mutually exclusive screen or editor action chain.
    let protected = KeymapCandidate::set(
        &source,
        &catalog(),
        &context("modal.confirm"),
        &action("confirm.cycle-focus"),
        &chords(&["Ctrl+Q"]),
        "settings.toml",
    )
    .err()
    .unwrap_or_else(|| panic!("modal override must not shadow protected global emergency-exit"));
    assert!(protected.to_string().contains("ProtectedShadowed"));
    assert_eq!(source.original_bytes(), original);
}

#[test]
fn invalid_protected_and_bounds_candidates_return_key_e401_without_publication() {
    let original = b"settings_schema = 2\n";
    let source = document(original);
    let protected = KeymapCandidate::unbind(
        &source,
        &catalog(),
        &context("global"),
        &action("core.emergency-exit"),
        "settings.toml",
    )
    .err()
    .unwrap_or_else(|| panic!("protected unbind must fail"));
    assert_eq!(
        protected.to_string().split(':').next(),
        Some(KeymapDiagnostic::code())
    );

    let over = KeymapCandidate::set(
        &source,
        &catalog(),
        &context("dashboard"),
        &action("dashboard.navigate-up"),
        &chords(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
        "settings.toml",
    )
    .err()
    .unwrap_or_else(|| panic!("nine chords must fail"));
    assert_eq!(
        over.to_string().split(':').next(),
        Some(KeymapDiagnostic::code())
    );
    assert_eq!(source.original_bytes(), original);
}

#[test]
fn malformed_keymap_publication_retains_valid_non_keymap_settings() {
    let source = br#"settings_schema = 2
[appearance]
theme = "green-screen"
override_agent_theme = true
[workbench]
initial_screen = "core.dashboard"
enabled_screens = ["core.dashboard"]
screen_order = ["core.dashboard"]
[agents."core.llxprt"]
enabled = false
[keymap.dashboard]
"dashboard.navigate-down" = 7
"#;
    let loaded = load_bytes(Some(source), &catalog(), "settings.toml")
        .unwrap_or_else(|diagnostics| panic!("keymap-only failure must recover: {diagnostics:?}"));

    assert_eq!(
        loaded.settings.appearance.theme.as_deref(),
        Some("green-screen")
    );
    assert_eq!(loaded.settings.appearance.override_agent_theme, Some(true));
    assert_eq!(
        loaded
            .settings
            .workbench
            .initial_screen
            .as_ref()
            .map(crate::domain::Id::as_str),
        Some("core.dashboard")
    );
    let agent = crate::domain::Id::parse("core.llxprt")
        .unwrap_or_else(|error| panic!("agent id fixture: {error}"));
    assert_eq!(
        loaded
            .settings
            .agents
            .get(&agent)
            .and_then(|owner| owner.enabled),
        Some(false)
    );
    assert!(loaded.settings.keymap.is_empty());
    assert!(loaded.diagnostic.is_some());
    let stack = ContextStack::from_ordered(["dashboard", "global"], false)
        .unwrap_or_else(|error| panic!("stack fixture: {error}"));
    assert!(matches!(
        loaded.composed.snapshot().resolve(&chords(&["j"])[0], &stack),
        Resolution::Dispatch { action, .. } if action.as_str() == "dashboard.navigate-down"
    ));
}

#[test]
fn malformed_settings_syntax_remains_fatal() {
    let source = b"settings_schema = 2\n[keymap.dashboard\n";
    let result = load_bytes(Some(source), &catalog(), "settings.toml");
    let Err(diagnostics) = result else {
        panic!("invalid TOML cannot isolate keymap and must remain fatal");
    };
    assert_eq!(diagnostics.len(), 1);
    assert!(!diagnostics[0].path.as_str().starts_with("/keymap"));
}

#[test]
fn revision_gate_retains_prior_bytes_when_candidate_becomes_stale() {
    let dir = std::env::temp_dir().join(format!(
        "jefe_keymap_edit_{}_{}",
        std::process::id(),
        counter()
    ));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let settings_path = dir.join("settings.toml");
    let original = b"settings_schema = 2\n";
    std::fs::write(&settings_path, original)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));
    let source = document(original);
    let candidate = KeymapCandidate::set(
        &source,
        &catalog(),
        &context("dashboard"),
        &action("dashboard.navigate-up"),
        &chords(&["w"]),
        "settings.toml",
    )
    .unwrap_or_else(|error| panic!("candidate must compose: {error}"));
    let manager = FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: settings_path.clone(),
        state_path: dir.join("state.json"),
    });

    let outcome = manager
        .save_keymap_candidate_revisioned(&candidate, 9, &|_| super::writer::Freshness::Stale)
        .unwrap_or_else(|error| panic!("stale write must be classified: {error}"));
    assert!(matches!(outcome, super::writer::WriteOutcome::Stale { .. }));
    assert_eq!(
        std::fs::read(&settings_path).unwrap_or_else(|error| panic!("read settings: {error}")),
        original
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn complete_multi_edit_candidate_validates_once_and_preserves_dormant_bytes() {
    use super::keymap_edit::KeymapEdit;
    use super::writer::ExpectedHash;

    let source = b"# heading\nsettings_schema = 2\n[keymap.global]\n\"core.open-keys\" = [\",\"] # keep\n[extensions.future]\nvalue = 'stay'\n";
    let document = document(source);
    let context = context("global");
    let action = action("core.open-keys");
    let chord = chords(&["."])
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("one chord fixture must exist"));
    let catalog = catalog();
    let candidate = KeymapCandidate::from_edits(
        &document,
        &catalog,
        &[KeymapEdit::set(context, action, vec![chord])],
        ExpectedHash::Present(document.sha256()),
        "settings",
    )
    .unwrap_or_else(|error| panic!("complete candidate must validate: {error}"));

    let rendered = String::from_utf8_lossy(candidate.bytes());
    assert!(rendered.contains("# heading"));
    assert!(rendered.contains("# keep"));
    assert!(rendered.contains("[extensions.future]\nvalue = 'stay'"));
    assert!(rendered.contains("\"core.open-keys\" = [\".\"]"));
}
