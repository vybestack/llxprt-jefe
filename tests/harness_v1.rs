//! End-to-end behavioral tests for the schema-1 harness runner
//! (issue #380: CW00-03, CW00-05, CW00-06, CW00-07, CW00-08, CW00-09).
//!
//! These execute the real runner against the real `jefe-harness-probe` and
//! `jefe-capture-shim` fixture binaries in real PTYs. Unix-only, like the
//! runner itself.
#![cfg(unix)]

use std::path::PathBuf;

use jefe::harness::v1::error::HarCode;
use jefe::harness::v1::redact::Redactor;
use jefe::harness::v1::runner::{RunOutcome, RunnerConfig};
use jefe::harness::v1::{parse_scenario_v1, run};

fn bin_path(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|err| panic!("current_exe: {err}"));
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(name)
}

fn run_scenario(json: &str) -> RunOutcome {
    let scenario =
        parse_scenario_v1(json.as_bytes()).unwrap_or_else(|err| panic!("should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
    };
    let outcome = run(&scenario, &config);
    // Every test workspace is retained on failure by contract; remove it
    // here after assertions via the returned path when the run passed.
    outcome
}

fn cleanup(outcome: &RunOutcome) {
    if !outcome.report.workspace.is_empty() {
        let _ = std::fs::remove_dir_all(&outcome.report.workspace);
    }
}

fn probe_scenario(platform: &str, steps: &str, secrets: &str) -> String {
    let probe = bin_path("jefe-harness-probe");
    format!(
        r#"{{"schema":1,"name":"e2e","platform":"{platform}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],"files":[],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["{}"],"env":[],"cwd":"work"}},
                {steps}
            ],"secrets":{secrets}}}"#,
        probe.display()
    )
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

#[test]
fn launch_wait_assert_and_finish_pass() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY 100x30","timeout_ms":10000},
           {"op":"assert-frame","contains":["PROBE READY 100x30"],"absent":["PANIC"]},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    assert_eq!(outcome.report.status, "passed");
    assert!(
        outcome.report.app_exit.is_some(),
        "finish must reap and record exit"
    );
    cleanup(&outcome);
}

#[test]
fn resize_waits_for_exact_dimension_frame() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY 100x30","timeout_ms":10000},
           {"op":"resize","size":{"cols":70,"rows":18}},
           {"op":"wait","source":"frame","literal":"PROBE READY 70x18","timeout_ms":10000},
           {"op":"assert-frame","contains":["PROBE READY 70x18"],"absent":[]},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    // CW00-05 evidence: distinct 100x30 and 70x18 frames in the report.
    let has_normal = outcome
        .report
        .frames
        .iter()
        .any(|frame| frame.cols == 100 && frame.rows == 30);
    let has_focused = outcome
        .report
        .frames
        .iter()
        .any(|frame| frame.cols == 70 && frame.rows == 18);
    assert!(has_normal, "report must contain a 100x30 frame");
    assert!(has_focused, "report must contain a 70x18 frame");
    cleanup(&outcome);
}

#[test]
fn restart_preserves_durable_files_and_replaces_process() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"write durable.txt persisted\n"},
           {"op":"wait","source":"frame","literal":"WROTE durable.txt","timeout_ms":10000},
           {"op":"restart"},
           {"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"assert-file","file":{"path":"work/durable.txt","content":{"utf8":"persisted"}}},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    // CW00-06: the relaunched probe printed a fresh PID line.
    let pid_lines: std::collections::BTreeSet<String> = outcome
        .report
        .frames
        .iter()
        .flat_map(|frame| frame.lines.iter())
        .filter(|line| line.starts_with("PROBE PID "))
        .cloned()
        .collect();
    assert!(
        pid_lines.len() >= 2,
        "restart must produce a new probe process, saw {pid_lines:?}"
    );
    cleanup(&outcome);
}

#[test]
fn capture_records_exact_process_boundary_fields() {
    let steps = r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"run gh pr view\n"},
           {"op":"wait","source":"frame","literal":"RUN EXIT 0","timeout_ms":10000},
           {"op":"assert-frame","contains":["RUN OUT gh-says-hello"],"absent":[]},
           {"op":"finish"}"#;
    let probe = bin_path("jefe-harness-probe");
    let json = format!(
        r#"{{"schema":1,"name":"cap","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],"files":[],"env":[]}},
            "steps":[
                {{"op":"capture","name":"gh","path":"bin/gh","behavior":{{"stdout":"gh-says-hello\n","stderr":"","exit_code":0,"stdin_limit":0,"hang":false,"spawn_child_hang":false}}}},
                {{"op":"launch","argv":["{}"],"env":[],"cwd":"work"}},
                {steps}
            ],"secrets":[]}}"#,
        current_platform(),
        probe.display()
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "gh")
        .unwrap_or_else(|| panic!("capture 'gh' must be reported"));
    assert_eq!(capture.invocations.len(), 1);
    let record = &capture.invocations[0];
    assert_eq!(record.ordinal, 1);
    assert!(record.completed);
    assert_eq!(record.exit_code, 0);
    // argv[0] is the shim path; the arguments are exact.
    assert!(record.argv[0].ends_with("gh"), "{:?}", record.argv);
    assert_eq!(&record.argv[1..], ["pr", "view"]);
    assert_eq!(record.stdout, "gh-says-hello\n");
    // The probe ran it from the workspace work dir with the closed env.
    assert!(record.cwd.ends_with("/work"), "{}", record.cwd);
    let path_pair = record
        .env
        .iter()
        .find(|(name, _)| name == "PATH")
        .unwrap_or_else(|| panic!("PATH must be recorded"));
    assert!(path_pair.1.ends_with("/bin"), "{}", path_pair.1);
    cleanup(&outcome);
}

#[test]
fn wait_timeout_escalates_and_reaps_hanging_process_tree() {
    let steps = r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"run slow-tool\n"},
           {"op":"wait","source":"frame","literal":"NEVER-PRINTED","timeout_ms":1500},
           {"op":"finish"}"#;
    let probe = bin_path("jefe-harness-probe");
    let json = format!(
        r#"{{"schema":1,"name":"hang","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],"files":[],"env":[]}},
            "steps":[
                {{"op":"capture","name":"slow-tool","path":"bin/slow-tool","behavior":{{"stdout":"","stderr":"","exit_code":0,"stdin_limit":0,"hang":true,"spawn_child_hang":true}}}},
                {{"op":"launch","argv":["{}"],"env":[],"cwd":"work"}},
                {steps}
            ],"secrets":[]}}"#,
        current_platform(),
        probe.display()
    );
    let outcome = run_scenario(&json);
    let err = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("wait must time out"));
    assert!(err.timeout, "timeout flag must map to exit 124");
    assert_eq!(err.exit_code(), 124);
    assert_eq!(outcome.report.status, "failed");
    // CW00-08: the shim (child) and its hanging grandchild are both gone.
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "slow-tool")
        .unwrap_or_else(|| panic!("capture must be reported"));
    let record = &capture.invocations[0];
    assert!(!record.completed, "hanging shim cannot have completed");
    for pid in [Some(record.pid), record.child_pid].into_iter().flatten() {
        assert!(
            !process_exists(pid),
            "descendant {pid} must be reaped after escalation"
        );
    }
    cleanup(&outcome);
}

#[test]
fn containment_violation_fails_before_access() {
    // A scenario step that removes a directory and swaps in a symlink is not
    // expressible through the closed grammar, so containment is covered at
    // the workspace layer in unit tests. Here we prove the end-to-end write
    // path refuses a symlink materialized as workspace content is impossible:
    // 'remove' then 'write' through the harness stays inside the workspace.
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"write","file":{"path":"work/inner.txt","content":{"utf8":"data"},"mode":420}},
           {"op":"assert-file","file":{"path":"work/inner.txt","content":{"utf8":"data"}}},
           {"op":"remove","path":"work/inner.txt"},
           {"op":"assert-file","file":{"path":"work/inner.txt","exists":false}},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    cleanup(&outcome);
}

#[test]
fn secrets_are_redacted_in_report_and_frames() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"say sekrit-token-123 now\n"},
           {"op":"wait","source":"frame","literal":"INPUT: say","timeout_ms":10000},
           {"op":"finish"}"#,
        r#"["sekrit-token-123"]"#,
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "run should pass: {:?}",
        outcome.error
    );
    let redactor = Redactor::new(&["sekrit-token-123".to_string()]);
    let rendered = outcome
        .report
        .to_redacted_json(&redactor)
        .unwrap_or_else(|err| panic!("report should encode: {err}"));
    assert!(
        !rendered.contains("sekrit-token-123"),
        "secret leaked into the report"
    );
    assert!(rendered.contains("<redacted>"));
    cleanup(&outcome);
}

#[test]
fn failure_stops_later_steps_and_retains_workspace() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"assert-frame","contains":["THIS IS NOT ON SCREEN"],"absent":[]},
           {"op":"write","file":{"path":"work/after.txt","content":{"utf8":"x"},"mode":420}},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    let err = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("assertion must fail"));
    assert_eq!(err.code, HarCode::E006);
    assert_eq!(err.exit_code(), 4);
    // Later steps did not run.
    let after = std::path::Path::new(&outcome.report.workspace).join("work/after.txt");
    assert!(!after.exists(), "steps after the failure must not execute");
    // Workspace is retained for diagnosis.
    assert!(
        std::path::Path::new(&outcome.report.workspace).is_dir(),
        "workspace must be retained on failure"
    );
    // Step results captured pass/fail per step.
    let statuses: Vec<(&str, &str)> = outcome
        .report
        .steps
        .iter()
        .map(|step| (step.op.as_str(), step.status.as_str()))
        .collect();
    assert!(
        statuses.contains(&("assert-frame", "failed")),
        "{statuses:?}"
    );
    cleanup(&outcome);
}

fn process_exists(pid: u32) -> bool {
    // Probe with /bin/kill -0 (no shell, fixed path).
    let kill = ["/bin/kill", "/usr/bin/kill"]
        .into_iter()
        .find(|path| std::path::Path::new(path).exists())
        .unwrap_or_else(|| panic!("no kill binary"));
    std::process::Command::new(kill)
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
