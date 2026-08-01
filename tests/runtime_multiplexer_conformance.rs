//! Classifying a multiplexer's answers against the declared contract
//! (issue #540 slice S2).
//!
//! The runner's whole job is to tell three outcomes apart: the binary honours
//! an item, the binary lacks an optional item, and the binary is not the
//! multiplexer jefe requires. Conflating the middle case with either of the
//! others is how jefe would either reject every released psmux or depend on a
//! format that renders as nothing.

use jefe::runtime::{
    ConformanceVerdict, ContractItemKind, ProbeOutcome, ProbePlan, classify_contract_probe,
    contract_item, contract_items, probe_plan_for, summarize_conformance,
};

const SCRATCH: &str = "jefe-conformance-scratch";

fn probe(exit_code: i32, stdout: &str) -> ProbeOutcome {
    ProbeOutcome {
        exit_code: Some(exit_code),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

fn format_item(name: &str) -> &'static jefe::runtime::ContractItem {
    contract_item(ContractItemKind::Format, name)
        .unwrap_or_else(|| panic!("{name} must be declared in the contract"))
}

fn verb_item(name: &str) -> &'static jefe::runtime::ContractItem {
    contract_item(ContractItemKind::Verb, name)
        .unwrap_or_else(|| panic!("{name} must be declared in the contract"))
}

/// A multiplexer predating psmux#509 renders `#{server_instance}` as empty text
/// and still exits zero. That is a missing optional capability, not a broken
/// binary, and must not fail qualification.
#[test]
fn an_absent_optional_format_is_unsupported_rather_than_a_failure() {
    let verdict = classify_contract_probe(format_item("server_instance"), &probe(0, ""));

    assert_eq!(
        verdict,
        ConformanceVerdict::Unsupported,
        "an empty render of a capability-gated format means the build predates it",
    );
}

/// The same empty answer for a format every multiplexer must provide means the
/// binary is not what jefe requires.
#[test]
fn an_absent_required_format_fails_qualification() {
    let verdict = classify_contract_probe(format_item("pane_pid"), &probe(0, ""));

    assert!(
        matches!(verdict, ConformanceVerdict::Violated { .. }),
        "a required format rendering empty must fail, got {verdict:?}",
    );
}

/// A format that answers is satisfied regardless of capability gating.
#[test]
fn a_format_that_answers_is_satisfied() {
    assert_eq!(
        classify_contract_probe(
            format_item("server_instance"),
            &probe(0, "19cd066a5ec1d650")
        ),
        ConformanceVerdict::Satisfied,
    );
    assert_eq!(
        classify_contract_probe(format_item("pane_pid"), &probe(0, "22440")),
        ConformanceVerdict::Satisfied,
    );
}

/// `has-session` reports through its exit status, so a non-zero exit is a
/// legitimate answer about a missing session rather than a contract violation.
/// Only a failure to understand the verb is a violation.
#[test]
fn a_verb_answering_by_exit_status_is_satisfied_even_when_it_reports_absence() {
    assert_eq!(
        classify_contract_probe(verb_item("has-session"), &probe(1, "")),
        ConformanceVerdict::Satisfied,
        "exit 1 from has-session means 'no such session', not 'verb unsupported'",
    );
}

/// A multiplexer that does not recognise a verb fails qualification.
#[test]
fn an_unrecognised_verb_fails_qualification() {
    let outcome = ProbeOutcome {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "unknown command: unbind-key".to_owned(),
    };

    assert!(
        matches!(
            classify_contract_probe(verb_item("unbind-key"), &outcome),
            ConformanceVerdict::Violated { .. }
        ),
        "an unknown command must fail qualification",
    );
}

/// A binary that could not be executed at all is a violation, not an absence.
#[test]
fn a_binary_that_never_ran_fails_qualification() {
    let outcome = ProbeOutcome {
        exit_code: None,
        stdout: String::new(),
        stderr: "program not found".to_owned(),
    };

    assert!(
        matches!(
            classify_contract_probe(verb_item("has-session"), &outcome),
            ConformanceVerdict::Violated { .. }
        ),
        "failing to run the binary must never be reported as an optional absence",
    );
}

/// A build lacking only optional items still qualifies, and the summary says so
/// without claiming the optional item is present.
#[test]
fn a_build_missing_only_optional_items_still_qualifies() {
    let findings = vec![
        (verb_item("has-session"), ConformanceVerdict::Satisfied),
        (format_item("pane_pid"), ConformanceVerdict::Satisfied),
        (
            format_item("server_instance"),
            ConformanceVerdict::Unsupported,
        ),
    ];

    let report = summarize_conformance(findings);

    assert!(
        report.is_qualified(),
        "a missing optional capability must not disqualify a build",
    );
    assert!(
        !report.supports("server_instance"),
        "an unsupported item must not be reported as available",
    );
    assert!(report.supports("pane_pid"));
}

/// A build violating a required item does not qualify, and the report names
/// every violation so the refusal can be actionable.
#[test]
fn a_build_violating_a_required_item_does_not_qualify() {
    let findings = vec![
        (verb_item("has-session"), ConformanceVerdict::Satisfied),
        (
            format_item("pane_pid"),
            ConformanceVerdict::Violated {
                detail: "rendered empty".to_owned(),
            },
        ),
    ];

    let report = summarize_conformance(findings);

    assert!(!report.is_qualified());
    let violations = report.violations();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].name, "pane_pid");
}

/// The runner owns a throwaway namespace, so every session-addressed probe must
/// name that scratch session. A probe that omitted the target would be
/// evaluated against whatever session the multiplexer considers current --
/// which, in the caller's namespace, is a live agent.
#[test]
fn every_session_addressed_probe_names_the_scratch_session() {
    for item in contract_items() {
        let ProbePlan::Command { args } = probe_plan_for(item, SCRATCH) else {
            continue;
        };
        if let Some(index) = args.iter().position(|arg| arg == "-t") {
            assert_eq!(
                args.get(index + 1).map(String::as_str),
                Some(SCRATCH),
                "`{}` targets something other than the scratch session",
                item.name,
            );
        }
    }
}

/// Attaching needs a controlling terminal, so probing it non-interactively
/// would block rather than answer.
#[test]
fn attaching_is_not_probed_non_interactively() {
    let item = contract_item(ContractItemKind::Verb, "attach-session")
        .unwrap_or_else(|| panic!("attach-session must be declared"));

    assert_eq!(probe_plan_for(item, SCRATCH), ProbePlan::RequiresTerminal);
}

/// A format probe must ask for that format and nothing else, or a passing
/// verdict would be evidence about the wrong variable.
#[test]
fn a_format_probe_requests_exactly_that_format() {
    let item = contract_item(ContractItemKind::Format, "pane_pid")
        .unwrap_or_else(|| panic!("pane_pid must be declared"));

    let ProbePlan::Command { args } = probe_plan_for(item, SCRATCH) else {
        panic!("a format must be probed by command");
    };

    assert!(args.contains(&"#{pane_pid}".to_owned()), "got {args:?}");
}
