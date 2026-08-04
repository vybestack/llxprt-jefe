//! Behavioral tests for provider-free binding explanation.

use std::sync::atomic::{AtomicU64, Ordering};

use super::binding_explain::run;

fn unique_dir(label: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jefe_binding_explain_{label}_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create explain fixture: {error}"));
    dir
}

#[test]
fn explain_reports_normalized_resolution_context_shadow_and_provenance() {
    let dir = unique_dir("override");
    let source =
        b"settings_schema = 2\n[keymap.dashboard]\n\"dashboard.navigate-down\" = [\"Ctrl+j\"]\n";
    std::fs::write(dir.join("settings.toml"), source)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));

    let output = run("Ctrl+j", Some("dashboard"), Some(&dir));

    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("normalized chord: Ctrl+J"));
    assert!(
        output
            .stdout
            .contains("searched contexts: dashboard -> global")
    );
    assert!(output.stdout.contains("winner: dashboard.navigate-down"));
    assert!(output.stdout.contains("resolution: dispatch"));
    assert!(output.stdout.contains("availability: available"));
    assert!(output.stdout.contains("reason: none"));
    assert!(output.stdout.contains("shadows: none"));
    assert!(output.stdout.contains("provenance: settings:"));
    assert_eq!(
        std::fs::read(dir.join("settings.toml")).unwrap_or_default(),
        source
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_uses_complete_snapshot_context_order() {
    let dir = unique_dir("nested-order");

    let output = run("Ctrl+Enter", Some("issues.inline"), Some(&dir));

    assert_eq!(output.exit_code, 0);
    assert!(
        output
            .stdout
            .contains("searched contexts: issues.inline -> global")
    );
    assert!(output.stdout.contains("winner: issues.inline-submit"));
    assert!(output.stdout.contains("context: issues.inline"));
    assert!(output.stdout.contains("provenance: compiled"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn canonical_alias_resolution_reports_lower_precedence_shadow() {
    let dir = unique_dir("shadow");
    // S4 introduces many finer-grained contexts whose compiled BackTab bindings
    // would be implicitly shadowed by a global override. The fixture overrides
    // each child explicitly so the composition accepts the candidate while still
    // exercising canonical alias resolution and shadow reporting.
    let source = b"settings_schema = 2\n[keymap.dashboard]\n\"dashboard.navigate-down\" = [\"BackTab\"]\n[keymap.split]\n\"split.cycle-pane\" = [\"BackTab\"]\n[keymap.errors]\n\"errors.cycle-pane\" = [\"BackTab\"]\n[keymap.global]\n\"core.open-keys\" = [\"BackTab\"]\n[keymap.\"modal.confirm\"]\n\"confirm.cycle-focus\" = [\"BackTab\"]\n[keymap.\"modal.form\"]\n\"form.previous-field\" = [\"BackTab\"]\n[keymap.\"issues.repo-list\"]\n\"issues.repo-cycle-pane\" = [\"BackTab\"]\n[keymap.\"issues.list\"]\n\"issues.list-cycle-pane\" = [\"BackTab\"]\n[keymap.\"issues.detail\"]\n\"issues.detail-subfocus-previous\" = [\"BackTab\"]\n[keymap.\"issues.new-form\"]\n\"issues.new-previous\" = [\"BackTab\"]\n[keymap.\"issues.filter\"]\n\"issues.filter-previous\" = [\"BackTab\"]\n[keymap.\"prs.repo-list\"]\n\"prs.repo-cycle-pane\" = [\"BackTab\"]\n[keymap.\"prs.list\"]\n\"prs.list-cycle-pane\" = [\"BackTab\"]\n[keymap.\"prs.detail\"]\n\"prs.detail-previous\" = [\"BackTab\"]\n[keymap.\"prs.changes\"]\n\"prs.changes-focus-files\" = [\"BackTab\"]\n[keymap.\"prs.filter\"]\n\"prs.filter-previous\" = [\"BackTab\"]\n[keymap.\"prs.new-form\"]\n\"prs.new-previous\" = [\"BackTab\"]\n[keymap.\"actions.filter\"]\n\"actions.filter-previous\" = [\"BackTab\"]\n";
    std::fs::write(dir.join("settings.toml"), source)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));

    let output = run("Shift+Tab", Some("dashboard"), Some(&dir));
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("winner: dashboard.navigate-down"));
    assert!(output.stdout.contains("shadows: global:core.open-keys"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn malformed_keymap_reports_key_e401_and_resolves_compiled_default() {
    let dir = unique_dir("fallback");
    let source =
        b"settings_schema = 2\n[keymap.dashboard]\n\"dashboard.navigate-down\" = [\"Ctrl+\"]\n";
    std::fs::write(dir.join("settings.toml"), source)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));

    let output = run("j", Some("dashboard"), Some(&dir));

    assert_eq!(output.exit_code, 0);
    assert!(output.stderr.contains("KEY-E401"));
    assert!(output.stdout.contains("winner: dashboard.navigate-down"));
    assert!(output.stdout.contains("provenance: compiled"));
    assert_eq!(
        std::fs::read(dir.join("settings.toml")).unwrap_or_default(),
        source
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_and_unresolved_inputs_exit_two_without_writes() {
    let dir = unique_dir("invalid");
    let invalid = run("Ctrl+", Some("dashboard"), Some(&dir));
    assert_eq!(invalid.exit_code, 2);
    assert!(invalid.stderr.contains("KEY-E401"));

    let unresolved = run("F24", Some("dashboard"), Some(&dir));
    assert_eq!(unresolved.exit_code, 2);
    assert!(unresolved.stdout.contains("resolution: unbound"));
    assert!(!dir.join("settings.toml").exists());
    assert!(!dir.join("state.json").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn malformed_settings_syntax_is_fatal_without_compiled_output() {
    let dir = unique_dir("fatal-syntax");
    let source = b"settings_schema = 2\n[keymap.dashboard\n";
    std::fs::write(dir.join("settings.toml"), source)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));

    let output = run("j", Some("dashboard"), Some(&dir));

    assert_eq!(output.exit_code, 2);
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(
        std::fs::read(dir.join("settings.toml")).unwrap_or_default(),
        source
    );
    let _ = std::fs::remove_dir_all(dir);
}
