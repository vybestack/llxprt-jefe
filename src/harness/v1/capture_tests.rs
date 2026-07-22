//! Behavioral tests for capture registration and expectation checks
//! (issue #380, CW00-03). Shim-process end-to-end coverage lives in
//! `tests/harness_v1.rs` where the fixture binary is available.

use super::super::contract::{CaptureExpectation, EnvVar};
use super::super::error::HarCode;
use super::{CaptureRecord, check_expectation};

fn record(ordinal: u64) -> CaptureRecord {
    CaptureRecord {
        ordinal,
        pid: 100,
        child_pid: None,
        argv: vec!["gh".to_string(), "pr".to_string(), "view".to_string()],
        env: vec![
            ("HOME".to_string(), "/ws/home".to_string()),
            ("PATH".to_string(), "/ws/bin".to_string()),
        ],
        cwd: "/ws/work".to_string(),
        stdin: "body".to_string(),
        stdout: "out".to_string(),
        stderr: "err".to_string(),
        exit_code: 0,
        completed: true,
    }
}

fn expectation() -> CaptureExpectation {
    CaptureExpectation {
        name: "gh".to_string(),
        invocation: 1,
        argv: vec!["gh".to_string(), "pr".to_string(), "view".to_string()],
        env: vec![
            EnvVar {
                name: "PATH".to_string(),
                value: "${workspace}/bin".to_string(),
            },
            EnvVar {
                name: "HOME".to_string(),
                value: "${workspace}/home".to_string(),
            },
        ],
        cwd: "${workspace}/work".to_string(),
        stdin: Some("body".to_string()),
        stdout: Some("out".to_string()),
        stderr: Some("err".to_string()),
        exit_code: Some(0),
        signal: None,
    }
}

#[test]
fn workspace_prefixed_records_match_token_expectations() {
    // Recorded values are absolute (/ws/...); expectations state the
    // ${workspace} token; unsorted expected env order is fine.
    check_expectation(&[record(1)], &expectation(), "/ws")
        .unwrap_or_else(|err| panic!("should pass: {err}"));
}

#[test]
fn values_outside_workspace_compare_verbatim() {
    let mut rec = record(1);
    rec.cwd = "/opt/elsewhere".to_string();
    let mut exp = expectation();
    exp.cwd = "/opt/elsewhere".to_string();
    check_expectation(&[rec], &exp, "/ws").unwrap_or_else(|err| panic!("should pass: {err}"));
}

#[test]
fn unlisted_env_pairs_are_permitted_listed_must_match() {
    let mut exp = expectation();
    exp.env.remove(0);
    check_expectation(&[record(1)], &exp, "/ws")
        .unwrap_or_else(|err| panic!("subset should pass: {err}"));
    let mut exp = expectation();
    exp.env.push(crate::harness::v1::contract::EnvVar {
        name: "MISSING".to_string(),
        value: "x".to_string(),
    });
    let err = check_expectation(&[record(1)], &exp, "/ws")
        .err()
        .unwrap_or_else(|| panic!("missing listed pair must fail"));
    assert_eq!(err.code, HarCode::E006);
}

type Mutation = fn(&mut CaptureExpectation);

#[test]
fn each_field_mismatch_is_e006() {
    let mutations: &[(&str, Mutation)] = &[
        ("argv", |e| {
            e.argv = vec!["gh".to_string(), "issue".to_string()];
        }),
        ("env", |e| e.env[0].value = "/other".to_string()),
        ("cwd", |e| e.cwd = "/elsewhere".to_string()),
        ("stdin", |e| e.stdin = Some("x".to_string())),
        ("stdout", |e| e.stdout = Some("x".to_string())),
        ("stderr", |e| e.stderr = Some("x".to_string())),
        ("exit_code", |e| e.exit_code = Some(3)),
        ("signal", |e| e.signal = Some(9)),
    ];
    for (field, mutate) in mutations {
        let mut exp = expectation();
        mutate(&mut exp);
        let err = check_expectation(&[record(1)], &exp, "/ws")
            .err()
            .unwrap_or_else(|| panic!("{field} mismatch must fail"));
        assert_eq!(err.code, HarCode::E006, "{field}");
        assert!(err.detail.contains(field), "{field}: {}", err.detail);
    }
}

#[test]
fn missing_invocation_is_e006() {
    let mut exp = expectation();
    exp.invocation = 2;
    let err = check_expectation(&[record(1)], &exp, "/ws")
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E006);
}

#[test]
fn optional_fields_skip_comparison_when_absent() {
    let mut exp = expectation();
    exp.stdin = None;
    exp.stdout = None;
    exp.stderr = None;
    exp.exit_code = None;
    let mut rec = record(1);
    rec.stdin = "different".to_string();
    rec.stdout = "different".to_string();
    rec.stderr = "different".to_string();
    rec.exit_code = 42;
    check_expectation(&[rec], &exp, "/ws").unwrap_or_else(|err| panic!("should pass: {err}"));
}

#[test]
fn incomplete_record_fails_exit_code_expectation() {
    let mut rec = record(1);
    rec.completed = false;
    let err = check_expectation(&[rec], &expectation(), "/ws")
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E006);
    assert!(err.detail.contains("did not complete"), "{}", err.detail);
}
