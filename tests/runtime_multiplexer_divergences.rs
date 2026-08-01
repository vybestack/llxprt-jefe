//! Declared behavioural divergences from tmux (issue #540 slice S6/V6).
//!
//! Each of these began as a patch applied where a symptom surfaced: a server
//! that vanished under jefe (#493), Page keys swallowed before reaching the
//! agent (#465), and jefe believing it was nested inside a parent session. A
//! patch records that something was wrong; it does not record what jefe
//! *requires*, so the next divergence is found the same way â€” in production.
//!
//! Declaring them turns each scar into a stated expectation the conformance
//! runner can assert, and gives the remediation a single definition instead of
//! a literal repeated wherever it was needed.

use jefe::runtime::{
    Divergence, declared_divergences, divergence, exit_empty_remediation, page_up_root_unbind,
    prefix_value_for_platform, psmux_session_routing_vars,
};

fn find(name: &str) -> &'static Divergence {
    divergence(name).unwrap_or_else(|| panic!("divergence `{name}` must be declared"))
}

/// A divergence with no issue behind it is folklore: nobody can tell whether it
/// is still needed or what would prove it fixed.
#[test]
fn every_divergence_cites_what_discovered_it() {
    let divergences = declared_divergences();
    assert!(!divergences.is_empty());

    for entry in divergences {
        assert!(
            !entry.discovered_by.is_empty(),
            "divergence `{}` must cite the issue that found it",
            entry.name,
        );
        assert!(
            !entry.expectation.is_empty(),
            "divergence `{}` must state what jefe requires",
            entry.name,
        );
        assert!(
            !entry.remediation.is_empty(),
            "divergence `{}` must state what jefe does about it",
            entry.name,
        );
    }
}

/// psmux servers exit when their last session closes, which tmux does not do in
/// jefe's configuration. Losing the server loses the namespace identity with
/// it, so the expectation is that a namespace outlives its sessions.
#[test]
fn the_server_must_outlive_its_last_session() {
    let entry = find("exit-empty");

    assert!(
        entry.discovered_by.contains("493"),
        "got {}",
        entry.discovered_by,
    );
    assert_eq!(
        exit_empty_remediation(),
        ["set-option", "-s", "exit-empty", "off"],
    );
}

/// psmux ships a default root-table `PageUp` binding that tmux does not. Left
/// in place it consumes the key before the agent sees it, so scrollback keys
/// never reach the child.
#[test]
fn page_keys_must_reach_the_child_unintercepted() {
    let entry = find("root-page-up-binding");

    assert!(
        entry.discovered_by.contains("465"),
        "got {}",
        entry.discovered_by,
    );
    assert_eq!(
        page_up_root_unbind(),
        ["unbind-key", "-T", "root", "PageUp"],
    );
}

/// psmux exports session-routing variables into the environment its children
/// inherit. A jefe process that inherits them believes it is running inside a
/// parent session and addresses the wrong one.
#[test]
fn session_routing_variables_must_not_be_inherited() {
    let entry = find("inherited-session-routing");

    assert_eq!(
        psmux_session_routing_vars(),
        ["PSMUX_SESSION", "PSMUX_TARGET_SESSION"],
    );
    assert!(
        entry.expectation.to_lowercase().contains("nested")
            || entry.expectation.to_lowercase().contains("inherit"),
        "the expectation must say what goes wrong: {}",
        entry.expectation,
    );
}

/// The team-mode and config variables are deliberately retained: they are not
/// session routing, and scrubbing them would change behaviour jefe wants.
#[test]
fn only_session_routing_variables_are_scrubbed() {
    let scrubbed = psmux_session_routing_vars();

    assert!(
        !scrubbed.contains(&"PSMUX_CLAUDE_TEAMMATE_MODE"),
        "team mode is not session routing and must survive: {scrubbed:?}",
    );
    assert!(
        !scrubbed.contains(&"PSMUX_CONFIG_FILE"),
        "the config variable is not session routing: {scrubbed:?}",
    );
}

/// An undeclared name must not resolve, so the declarations stay the single
/// list rather than one of several.
#[test]
fn an_undeclared_divergence_is_not_found() {
    assert!(divergence("remain-on-exit-forever").is_none());
}

/// The prefix override is version-specific and, per the issue, had "no
/// assertion that it is still true". Declaring it records the psmux behaviour
/// that forces the choice, so a future re-check has something to test against
/// rather than a bare constant.
#[test]
fn the_reserved_prefix_key_divergence_is_declared() {
    let entry = find("reserved-prefix-key");

    assert!(
        entry.discovered_by.contains("446"),
        "got {}",
        entry.discovered_by,
    );
    assert!(
        entry.expectation.contains("C-b"),
        "the expectation must name the key psmux keeps reserved: {}",
        entry.expectation,
    );
    assert!(
        entry.discovered_by.contains("3.3.6"),
        "a version-specific quirk must record the version it was seen on: {}",
        entry.discovered_by,
    );
}

/// The remediation applies to Windows only; tmux honours `None`, so releasing
/// the prefix there needs no substitute key.
#[test]
fn only_the_psmux_platform_needs_a_substitute_prefix() {
    assert_eq!(prefix_value_for_platform(true), "F12");
    assert_eq!(
        prefix_value_for_platform(false),
        "None",
        "tmux honours None, so no key needs to be spent there",
    );
}
