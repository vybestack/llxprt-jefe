//! Startup qualification and its refusal message (issue #540 slices V2/V3).
//!
//! The maintainer decision on this issue was to reject bundling psmux. That
//! makes the refusal itself the product surface: when jefe declines to run, the
//! message is the entire remedy the operator gets. It must therefore name the
//! binary it rejected, the version it found there, what specifically failed,
//! and what to do about it.

use std::path::PathBuf;

use jefe::runtime::{
    ConformanceVerdict, ContractItemKind, MultiplexerQualification, contract_item,
    qualification_from_report, summarize_conformance,
};

fn item(kind: ContractItemKind, name: &str) -> &'static jefe::runtime::ContractItem {
    contract_item(kind, name).unwrap_or_else(|| panic!("{name} must be declared"))
}

fn refusal_for(
    findings: Vec<(&'static jefe::runtime::ContractItem, ConformanceVerdict)>,
    version: Option<&str>,
) -> String {
    let report = summarize_conformance(findings);
    let qualification = qualification_from_report(
        &PathBuf::from(r"C:\Tools\psmux\psmux.exe"),
        version.map(str::to_owned),
        report,
    );

    match qualification {
        MultiplexerQualification::Refused { message } => message,
        MultiplexerQualification::Qualified { .. } => {
            panic!("expected a refusal for these findings")
        }
    }
}

fn violation(name: &str) -> ConformanceVerdict {
    ConformanceVerdict::Violated {
        detail: format!("{name} rendered empty"),
    }
}

/// The operator has to know which binary was rejected: the override and the
/// PATH lookup can resolve to different installs, and "psmux is unsupported"
/// without a path does not say which one to fix.
#[test]
fn a_refusal_names_the_binary_it_rejected() {
    let message = refusal_for(
        vec![(
            item(ContractItemKind::Format, "pane_pid"),
            violation("pane_pid"),
        )],
        Some("3.3.7"),
    );

    assert!(
        message.contains(r"C:\Tools\psmux\psmux.exe"),
        "refusal must name the rejected binary, got: {message}",
    );
}

/// Naming the version found is what turns "unsupported" into "you have X".
#[test]
fn a_refusal_names_the_version_it_found() {
    let message = refusal_for(
        vec![(
            item(ContractItemKind::Format, "pane_pid"),
            violation("pane_pid"),
        )],
        Some("3.3.7"),
    );

    assert!(
        message.contains("3.3.7"),
        "refusal must name the version found, got: {message}",
    );
}

/// A binary too broken to report a version must say so rather than imply one.
#[test]
fn a_refusal_admits_when_the_version_is_unknown() {
    let message = refusal_for(
        vec![(
            item(ContractItemKind::Verb, "has-session"),
            violation("has-session"),
        )],
        None,
    );

    assert!(
        message.to_lowercase().contains("unknown"),
        "an undetermined version must be stated, not omitted: {message}",
    );
}

/// Reporting only the first failure would send the operator round the loop
/// once per defect.
#[test]
fn a_refusal_names_every_failing_requirement() {
    let message = refusal_for(
        vec![
            (
                item(ContractItemKind::Format, "pane_pid"),
                violation("pane_pid"),
            ),
            (
                item(ContractItemKind::Verb, "unbind-key"),
                violation("unbind-key"),
            ),
        ],
        Some("3.3.7"),
    );

    assert!(message.contains("pane_pid"), "got: {message}");
    assert!(message.contains("unbind-key"), "got: {message}");
}

/// Since jefe will not install psmux for the operator, the refusal must say how
/// to supply a working one.
#[test]
fn a_refusal_states_the_remedy_including_the_override() {
    let message = refusal_for(
        vec![(
            item(ContractItemKind::Format, "pane_pid"),
            violation("pane_pid"),
        )],
        Some("3.3.7"),
    );

    assert!(
        message.contains("JEFE_PSMUX_BIN"),
        "refusal must name the override that lets an operator point at a good build: {message}",
    );
}

/// An optional capability this build predates is not a failure and must not
/// appear in the refusal, or the operator chases a defect that is not there.
#[test]
fn a_refusal_does_not_list_merely_unsupported_capabilities() {
    let message = refusal_for(
        vec![
            (
                item(ContractItemKind::Format, "pane_pid"),
                violation("pane_pid"),
            ),
            (
                item(ContractItemKind::Format, "server_instance"),
                ConformanceVerdict::Unsupported,
            ),
        ],
        Some("3.3.7"),
    );

    assert!(
        !message.contains("server_instance"),
        "an unsupported optional capability is not a failing requirement: {message}",
    );
}

/// A build honouring every required item qualifies, and carries its report so
/// callers can degrade against optional capabilities.
#[test]
fn a_conforming_build_qualifies_and_retains_its_report() {
    let report = summarize_conformance(vec![
        (
            item(ContractItemKind::Format, "pane_pid"),
            ConformanceVerdict::Satisfied,
        ),
        (
            item(ContractItemKind::Format, "server_instance"),
            ConformanceVerdict::Unsupported,
        ),
    ]);

    let qualification =
        qualification_from_report(&PathBuf::from("psmux"), Some("3.3.7".to_owned()), report);

    let MultiplexerQualification::Qualified { report } = qualification else {
        panic!("a build honouring every required item must qualify");
    };
    assert!(report.supports("pane_pid"));
    assert!(
        !report.supports("server_instance"),
        "qualification must not upgrade an unsupported capability to available",
    );
}
