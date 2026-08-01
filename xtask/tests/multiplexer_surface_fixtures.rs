//! Mechanical enforcement of the declared multiplexer surface (jefe #540, V5).
//!
//! The contract only stays true if the build refuses code that steps outside
//! it. These fixtures drive the policy over source text directly, so the check
//! is exercised without depending on the repository's current contents.

use std::collections::BTreeSet;

use xtask::multiplexer_surface::{Violation, declared_surface, format_usages, surface_violations};

/// A miniature stand-in for `src/runtime/multiplexer_contract.rs`.
const CONTRACT: &str = r#"
    format(
        "pane_pid",
        false,
        ContractCapability::Always,
        "PID of the pane leader",
    ),
    format(
        "server_instance",
        true,
        ContractCapability::SincePsmuxNamespaceToken,
        "stable namespace token",
    ),
"#;

fn declared() -> BTreeSet<String> {
    declared_surface(CONTRACT).formats
}

/// The contract is only authoritative if the declarations can be read back out
/// of it.
#[test]
fn the_declared_formats_are_read_from_the_contract_source() {
    let formats = declared();

    assert!(formats.contains("pane_pid"), "got {formats:?}");
    assert!(formats.contains("server_instance"), "got {formats:?}");
    assert_eq!(formats.len(), 2, "got {formats:?}");
}

/// Reaching for a format nobody declared is the drift this check exists to
/// stop: it is a dependency the conformance suite will never assert.
#[test]
fn a_format_used_without_being_declared_is_a_violation() {
    let used = format_usages("const F: &str = \"#{client_tty}\";");
    let violations = surface_violations(&declared(), &used);

    assert!(
        violations.iter().any(|violation| matches!(
            violation,
            Violation::UsedButNotDeclared { name, .. } if name == "client_tty"
        )),
        "got {violations:?}",
    );
}

/// A declaration nothing uses is equally wrong: it makes the conformance suite
/// demand a capability jefe does not need, which can reject a serviceable
/// binary.
#[test]
fn a_format_declared_without_being_used_is_a_violation() {
    let used = format_usages("const F: &str = \"#{pane_pid}\";");
    let violations = surface_violations(&declared(), &used);

    assert!(
        violations.iter().any(|violation| matches!(
            violation,
            Violation::DeclaredButNotUsed { name } if name == "server_instance"
        )),
        "got {violations:?}",
    );
}

/// Rust's own interpolation shares the `#{...}` spelling when a literal `#`
/// precedes a placeholder. `format!("... #{ordinal}")` names a variable in
/// scope, not a multiplexer format, and must not be mistaken for one.
#[test]
fn rust_interpolation_is_not_mistaken_for_a_multiplexer_format() {
    let source = r#"
        HarnessError::process(format!("read capture record '{name}' #{ordinal}: {err}"))
    "#;

    let used = format_usages(source);

    assert!(
        !used.contains("ordinal"),
        "a Rust format placeholder is not a multiplexer format: {used:?}",
    );
}

/// The same spelling used to build an agent's display identifier is also not a
/// multiplexer format.
#[test]
fn a_display_identifier_is_not_a_multiplexer_format() {
    let used = format_usages(r##"agent.display_id = format!("#{next_display_index}");"##);

    assert!(!used.contains("next_display_index"), "got {used:?}");
}

/// A source honouring the contract exactly produces no violations.
#[test]
fn a_conforming_source_has_no_violations() {
    let used =
        format_usages("const A: &str = \"#{pane_pid}\";\nconst B: &str = \"#{server_instance}\";");

    assert!(surface_violations(&declared(), &used).is_empty());
}

/// A `format!` whose literal sits on a later line cannot be recognised by the
/// macro name alone. A bare `{name}` placeholder alongside gives it away: a
/// multiplexer format spells every variable `#{...}`.
#[test]
fn a_rust_format_literal_on_its_own_line_is_still_not_a_multiplexer_format() {
    let used = format_usages("    \"read capture start record '{name}' #{ordinal}: {err}\"");

    assert!(
        !used.contains("ordinal"),
        "a literal carrying bare placeholders is a Rust format string: {used:?}",
    );
}

/// The discriminator must not reject genuine multiplexer formats, which spell
/// every variable with the `#` prefix.
#[test]
fn a_composite_multiplexer_format_is_still_recognised() {
    let used = format_usages(
        "const PANE_FORMAT: &str = \"#{session_name}:#{window_index}.#{pane_index}\";",
    );

    assert!(used.contains("session_name"), "got {used:?}");
    assert!(used.contains("window_index"), "got {used:?}");
    assert!(used.contains("pane_index"), "got {used:?}");
}
