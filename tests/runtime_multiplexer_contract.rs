//! The enumerated psmux/tmux contract surface (issue #540 slice S1).
//!
//! jefe treats psmux as a drop-in tmux and discovers the difference one
//! production incident at a time (#433 byte budget, #465 PageUp binding, #493
//! `exit-empty`, #540 `#{pid}` namespace semantics). These tests pin the
//! authoritative list of what jefe depends on, so a conformance runner (S2) can
//! assert each item against the live binary and a mechanical check (S5) can
//! fail the build on an undeclared use.

use jefe::runtime::{
    ContractCapability, ContractItemKind, ResponseShape, contract_item, contract_items,
};

/// Every format string production code issues must be declared, or the
/// conformance suite cannot assert its response shape.
#[test]
fn the_contract_declares_every_format_the_runtime_depends_on() {
    for name in [
        "pane_dead",
        "pane_dead_signal",
        "pane_index",
        "pane_pid",
        "pid",
        "server_instance",
        "session_name",
        "window_index",
        "window_name",
        "version",
    ] {
        assert!(
            contract_item(ContractItemKind::Format, name).is_some(),
            "format #{{{name}}} is used by the runtime but not declared in the contract",
        );
    }
}

/// Every verb production code issues must be declared.
#[test]
fn the_contract_declares_every_verb_the_runtime_issues() {
    for name in [
        "attach-session",
        "capture-pane",
        "display-message",
        "has-session",
        "kill-server",
        "kill-session",
        "list-panes",
        "list-sessions",
        "list-windows",
        "new-session",
        "new-window",
        "select-window",
        "send-keys",
        "set-option",
        "show-options",
        "unbind-key",
    ] {
        assert!(
            contract_item(ContractItemKind::Verb, name).is_some(),
            "verb `{name}` is issued by the runtime but not declared in the contract",
        );
    }
}

/// `#{server_instance}` only exists on builds carrying upstream psmux#509.
/// Declaring it unconditionally would make the conformance suite reject every
/// currently-released psmux; declaring it as always-present would let jefe
/// depend on a token that is not there (issue #540).
#[test]
fn the_namespace_token_is_gated_behind_its_upstream_capability() {
    let item = contract_item(ContractItemKind::Format, "server_instance")
        .unwrap_or_else(|| panic!("the namespace token must be declared"));

    assert_eq!(
        item.capability,
        ContractCapability::SincePsmuxNamespaceToken,
        "the namespace token must be capability-gated, not assumed present",
    );
}

/// `#{pid}` is always available but answers for whichever per-session server
/// replied, so the contract must not present it as a namespace identity.
#[test]
fn the_server_pid_is_always_available_but_never_a_namespace_identity() {
    let item = contract_item(ContractItemKind::Format, "pid")
        .unwrap_or_else(|| panic!("#{{pid}} must be declared"));

    assert_eq!(item.capability, ContractCapability::Always);
    assert!(
        !item.namespace_stable,
        "#{{pid}} names the answering server process, not the -L namespace",
    );

    let token = contract_item(ContractItemKind::Format, "server_instance")
        .unwrap_or_else(|| panic!("the namespace token must be declared"));
    assert!(
        token.namespace_stable,
        "the namespace token is the only namespace-stable identity jefe has",
    );
}

/// A verb jefe never issues must not be declared: the surface is the
/// authoritative list, so padding it would blunt the S5 mechanical check.
#[test]
fn an_undeclared_name_is_not_found() {
    assert!(contract_item(ContractItemKind::Verb, "list-buffers").is_none());
    assert!(contract_item(ContractItemKind::Format, "client_tty").is_none());
}

/// Each declaration must carry a response shape, since that is what the
/// conformance runner asserts against the live binary.
#[test]
fn every_declared_item_states_its_response_shape() {
    let items = contract_items();
    assert!(!items.is_empty(), "the contract surface must not be empty");

    for item in items {
        assert!(
            !item.name.is_empty(),
            "a contract item must name the verb or format it governs",
        );
        assert!(
            !item.rationale.is_empty(),
            "contract item `{}` must record why jefe depends on it",
            item.name,
        );
        // The shape is what the runner routes on, so a format declared as
        // producing nothing would be judged by its exit status and its
        // substitution never checked -- the check would pass without ever
        // looking at the answer.
        if item.kind == ContractItemKind::Format {
            assert!(
                !matches!(
                    item.response,
                    ResponseShape::NoOutput | ResponseShape::ExitStatusOnly
                ),
                "format #{{{}}} declares a shape that produces no output, so its \
                 substitution would never be examined",
                item.name,
            );
        }
    }
}

/// `has-session` answers through its exit status, not stdout. Asserting that
/// here keeps the conformance runner from checking output that never comes.
#[test]
fn has_session_answers_through_exit_status() {
    let item = contract_item(ContractItemKind::Verb, "has-session")
        .unwrap_or_else(|| panic!("has-session must be declared"));

    assert_eq!(item.response, ResponseShape::ExitStatusOnly);
}
