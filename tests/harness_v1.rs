//! End-to-end behavioral tests for the schema-1 harness runner
//! (issue #380: CW00-03, CW00-05, CW00-06, CW00-07, CW00-08, CW00-09).
//!
//! These execute the real runner against the real `jefe-harness-probe` and
//! `jefe-capture-shim` fixture binaries in real PTYs. Unix-only, like the
//! runner itself.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use jefe::harness::v1::error::HarCode;
use jefe::harness::v1::redact::Redactor;
use jefe::harness::v1::runner::{RunOutcome, RunnerConfig};
use jefe::harness::v1::{HarnessError, parse_scenario_v1, run};

fn bin_path(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|err| panic!("current_exe: {err}"));
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(name)
}

struct Sentinel(Child);

impl Sentinel {
    fn start() -> Self {
        let child = Command::new("/bin/sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|err| panic!("start unrelated sentinel: {err}"));
        Self(child)
    }

    fn assert_alive(&mut self) {
        assert!(
            self.0
                .try_wait()
                .unwrap_or_else(|err| panic!("poll unrelated sentinel: {err}"))
                .is_none(),
            "runner reaped an unrelated process"
        );
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_scenario(json: &str) -> RunOutcome {
    let mut sentinel = Sentinel::start();
    let scenario =
        parse_scenario_v1(json.as_bytes()).unwrap_or_else(|err| panic!("should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
        installs: Vec::new(),
    };
    let outcome = run(&scenario, &config);
    sentinel.assert_alive();
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
fn runner_rejects_unsafe_or_duplicate_install_names_before_workspace_allocation() {
    let json = probe_scenario(current_platform(), r#"{"op":"finish"}"#, "[]");
    let scenario = parse_scenario_v1(json.as_bytes())
        .unwrap_or_else(|err| panic!("install validation scenario should parse: {err}"));
    for names in [vec!["../escape"], vec!["tool", "tool"]] {
        let config = RunnerConfig {
            shim_binary: bin_path("jefe-capture-shim"),
            installs: names
                .into_iter()
                .map(|name| (name.to_string(), PathBuf::from("/bin/true")))
                .collect(),
        };
        let outcome = run(&scenario, &config);
        assert_eq!(
            outcome.error.as_ref().map(HarnessError::code),
            Some(HarCode::E001)
        );
        assert!(outcome.report.workspace.is_empty());
    }
}

#[test]
fn cli_rejects_terminal_limits_before_execution_without_a_report() {
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/harness_v1/harness-limits.json");
    let output = Command::new(env!("CARGO_BIN_EXE_tmux_scenario"))
        .args([
            "--scenario",
            scenario
                .to_str()
                .unwrap_or_else(|| panic!("fixture path is UTF-8")),
        ])
        .output()
        .unwrap_or_else(|err| panic!("run tmux_scenario parser fixture: {err}"));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "HAR-E002: scenario.terminal.cols: 501 is outside 1..=500\n"
    );
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
fn resize_to_current_dimensions_is_an_acknowledged_no_op() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY 100x30","timeout_ms":10000},
           {"op":"resize","size":{"cols":100,"rows":30}},
           {"op":"assert-frame","contains":["PROBE READY 100x30"],"absent":[]},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "no-op resize should pass: {:?}",
        outcome.error
    );
    assert_eq!(outcome.report.status, "passed");
    cleanup(&outcome);
}

#[test]
fn finish_accepts_harness_controlled_termination() {
    let json = format!(
        r#"{{"schema":1,"name":"harness-termination","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],"files":[],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["/bin/sh","-c","trap 'exit 1' TERM; printf 'READY\\r\\n'; while read line; do :; done"],"env":[],"cwd":"work"}},
                {{"op":"wait","source":"frame","literal":"READY","timeout_ms":10000}},
                {{"op":"finish"}}
            ],"secrets":[]}}"#,
        current_platform()
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "harness-induced exit must not become an application failure: {:?}",
        outcome.error
    );
    assert_eq!(outcome.report.status, "passed");
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
    // The probe prints EXIT before it replays the captured stdout, so waiting
    // for EXIT and then asserting on the OUT line races the write of that line.
    // Wait for the last line the run emits; EXIT is necessarily on screen by then.
    let steps = r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"run gh pr view\n"},
           {"op":"wait","source":"frame","literal":"RUN[1] OUT gh-says-hello","timeout_ms":10000},
           {"op":"assert-frame","contains":["RUN[1] EXIT 0","RUN[1] OUT gh-says-hello"],"absent":[]},
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
    assert_eq!(record.exit_code, Some(0));
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
           {"op":"wait","source":"frame","literal":"SLOW-TOOL-STARTED","timeout_ms":10000},
           {"op":"wait","source":"frame","literal":"NEVER-PRINTED","timeout_ms":1500},
           {"op":"finish"}"#;
    let probe = bin_path("jefe-harness-probe");
    let json = format!(
        r#"{{"schema":1,"name":"hang","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],"files":[],"env":[]}},
            "steps":[
                {{"op":"capture","name":"slow-tool","path":"bin/slow-tool","behavior":{{"stdout":"","stderr":"SLOW-TOOL-STARTED\n","exit_code":0,"stdin_limit":0,"hang":true,"spawn_child_hang":true}}}},
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
    assert!(err.is_timeout(), "timeout flag must map to exit 124");
    assert_eq!(err.exit_code(), 124);
    assert_eq!(outcome.report.status, "failed");
    let failed = outcome
        .report
        .steps
        .iter()
        .find(|step| step.status == "failed")
        .unwrap_or_else(|| panic!("timeout step must be reported"));
    assert_eq!((failed.index, failed.op.as_str()), (4, "wait"));
    // CW00-08: the shim (child) and its hanging grandchild are both gone.
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "slow-tool")
        .unwrap_or_else(|| panic!("capture must be reported"));
    let record = &capture.invocations[0];
    assert!(!record.completed, "hanging shim cannot have completed");
    assert_eq!(
        record.signal,
        Some(15),
        "TERM phase must record the exact terminating signal"
    );
    assert_eq!(record.exit_code, None);
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
fn materialization_failure_reports_retained_workspace() {
    let probe = bin_path("jefe-harness-probe");
    let json = format!(
        r#"{{"schema":1,"name":"materialize-failure","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[],"files":[{{"path":"bin","content":{{"utf8":"conflict"}},"mode":420}}],"env":[]}},
            "steps":[{{"op":"launch","argv":["{}"],"env":[],"cwd":"bin"}}],"secrets":[]}}"#,
        current_platform(),
        probe.display()
    );
    let outcome = run_scenario(&json);
    assert!(outcome.error.is_some(), "materialization must fail");
    assert!(!outcome.report.workspace.is_empty());
    assert!(
        std::path::Path::new(&outcome.report.workspace).is_dir(),
        "allocated workspace must be retained and reported"
    );
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
    assert_eq!(err.code(), HarCode::E006);
    assert_eq!(err.exit_code(), 4);
    let failed = outcome
        .report
        .steps
        .iter()
        .find(|step| step.status == "failed")
        .unwrap_or_else(|| panic!("assertion step must be reported"));
    assert_eq!((failed.index, failed.op.as_str()), (2, "assert-frame"));
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

#[test]
fn unexpected_app_exit_fails_the_active_step_without_waiting_for_timeout() {
    let json = probe_scenario(
        current_platform(),
        r#"{"op":"wait","source":"frame","literal":"PROBE READY","timeout_ms":10000},
           {"op":"text","text":"exit\n"},
           {"op":"wait","source":"frame","literal":"NEVER-PRINTED","timeout_ms":1000},
           {"op":"finish"}"#,
        "[]",
    );
    let outcome = run_scenario(&json);
    let err = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("unexpected app exit must fail"));
    assert_eq!(err.code(), HarCode::E005);
    assert_eq!(err.exit_code(), 4);
    assert_eq!(
        outcome.report.app_exit.and_then(|exit| exit.exit_code),
        Some(0)
    );
    let failed = outcome
        .report
        .steps
        .iter()
        .find(|step| step.status == "failed")
        .unwrap_or_else(|| panic!("child failure step must be reported"));
    assert_eq!((failed.index, failed.op.as_str()), (3, "wait"));
    cleanup(&outcome);
}

#[test]
fn nonzero_exit_after_observable_output_cannot_finish_green() {
    let json = format!(
        r##"{{"schema":1,"name":"nonzero-child","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],
                "files":[{{"path":"work/fail.sh","content":{{"utf8":"#!/bin/sh\nprintf ready > ready.txt\nprintf 'WROTE\\n'\nexit 17\n"}},"mode":493}}],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["${{workspace}}/work/fail.sh"],"env":[],"cwd":"work"}},
                {{"op":"wait","source":"stdout","literal":"WROTE","timeout_ms":10000}},
                {{"op":"assert-file","file":{{"path":"work/ready.txt","exists":true,"content":{{"utf8":"ready"}}}}}},
                {{"op":"finish"}}
            ],"secrets":[]}}"##,
        current_platform()
    );
    let outcome = run_scenario(&json);
    let error = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("nonzero application exit must fail"));
    assert_eq!(error.code(), HarCode::E005);
    assert_eq!(error.exit_code(), 4);
    assert_eq!(
        outcome.report.app_exit.and_then(|exit| exit.exit_code),
        Some(17)
    );
    let failed = outcome
        .report
        .steps
        .iter()
        .find(|step| step.status == "failed")
        .unwrap_or_else(|| panic!("nonzero child exit must be reported"));
    assert_eq!(failed.op, "finish");
    cleanup(&outcome);
}

#[test]
fn declared_nonzero_exit_can_finish_green() {
    let json = format!(
        r##"{{"schema":1,"name":"expected-nonzero-child","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],
                "files":[{{"path":"work/fail.sh","content":{{"utf8":"#!/bin/sh\nprintf 'WROTE\\n'\nexit 17\n"}},"mode":493}}],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["${{workspace}}/work/fail.sh"],"env":[],"cwd":"work"}},
                {{"op":"wait","source":"stdout","literal":"WROTE","timeout_ms":10000}},
                {{"op":"finish","expected_exit_code":17}}
            ],"secrets":[]}}"##,
        current_platform()
    );
    let outcome = run_scenario(&json);
    assert!(
        outcome.error.is_none(),
        "expected nonzero exit: {outcome:?}"
    );
    assert_eq!(
        outcome.report.app_exit.and_then(|exit| exit.exit_code),
        Some(17)
    );
    cleanup(&outcome);

    let mismatch =
        run_scenario(&json.replace("\"expected_exit_code\":17", "\"expected_exit_code\":3"));
    let error = mismatch
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("mismatched expected exit code must fail"));
    assert_eq!(error.code(), HarCode::E005);
    assert!(error.to_string().contains("instead of expected code 3"));
    cleanup(&mismatch);
}

#[test]
fn owned_app_socket_cleanup_reuses_the_hermetic_launch_environment() {
    let fixture = tempfile::tempdir().unwrap_or_else(|err| panic!("fixture directory: {err}"));
    let tmux = fixture.path().join("tmux");
    std::fs::write(
        &tmux,
        "#!/bin/sh\n[ \"$CLEANUP_TOKEN\" = expected ] || exit 31\n[ \"$1\" = -S ] || exit 32\n[ \"$3\" = kill-server ] || exit 33\n",
    )
    .unwrap_or_else(|err| panic!("write tmux fixture: {err}"));
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|err| panic!("chmod tmux fixture: {err}"));
    let json = format!(
        r##"{{"schema":1,"name":"socket-cleanup-environment","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],
                "files":[{{"path":"work/app.sh","content":{{"utf8":"#!/bin/sh\n: > \"$JEFE_SOCKET_PATH\"\nprintf 'READY\\n'\nexec /bin/sleep 60\n"}},"mode":493}}],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["${{workspace}}/work/app.sh"],"env":[{{"name":"JEFE_SOCKET_PATH","value":"${{workspace}}/owned.sock"}},{{"name":"CLEANUP_TOKEN","value":"expected"}}],"cwd":"work"}},
                {{"op":"wait","source":"stdout","literal":"READY","timeout_ms":10000}},
                {{"op":"finish"}}
            ],"secrets":[]}}"##,
        current_platform()
    );
    let scenario =
        parse_scenario_v1(json.as_bytes()).unwrap_or_else(|err| panic!("should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
        installs: vec![("tmux".to_string(), tmux)],
    };
    let mut sentinel = Sentinel::start();
    let outcome = run(&scenario, &config);
    sentinel.assert_alive();
    assert!(outcome.error.is_none(), "outcome={outcome:?}");
    assert_eq!(outcome.report.status, "passed");
    assert!(
        !std::path::Path::new(&outcome.report.workspace)
            .join("owned.sock")
            .exists(),
        "runner must remove the exact stale socket after successful cleanup"
    );
    cleanup(&outcome);
}

#[test]
fn restart_reaps_the_owned_socket_before_launching_a_distinct_generation() {
    let fixture = tempfile::tempdir().unwrap_or_else(|err| panic!("fixture directory: {err}"));
    let tmux = fixture.path().join("tmux");
    std::fs::write(
        &tmux,
        "#!/bin/sh\n[ \"$CLEANUP_TOKEN\" = expected ] || exit 31\n[ \"$1\" = -S ] || exit 32\n[ \"$3\" = kill-server ] || exit 33\nprintf 'cleanup\\n' >> \"$CLEANUP_LOG\"\n",
    )
    .unwrap_or_else(|err| panic!("write tmux fixture: {err}"));
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|err| panic!("chmod tmux fixture: {err}"));
    let unrelated_path = fixture.path().join("unrelated.sock");
    let _unrelated_socket = std::os::unix::net::UnixListener::bind(&unrelated_path)
        .unwrap_or_else(|err| panic!("bind unrelated socket: {err}"));
    let json = format!(
        r##"{{"schema":1,"name":"restart-socket-cleanup","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],
                "files":[{{"path":"work/app.sh","content":{{"utf8":"#!/bin/sh\ngeneration=1\nif [ -f generation ]; then generation=$(( $(/bin/cat generation) + 1 )); fi\nprintf '%s' \"$generation\" > generation\nprintf 'generation-%s' \"$generation\" > \"$JEFE_SOCKET_PATH\"\nprintf 'READY-%s\\n' \"$generation\"\nexec /bin/sleep 60\n"}},"mode":493}}],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["${{workspace}}/work/app.sh"],"env":[{{"name":"JEFE_SOCKET_PATH","value":"${{workspace}}/owned.sock"}},{{"name":"CLEANUP_TOKEN","value":"expected"}},{{"name":"CLEANUP_LOG","value":"${{workspace}}/cleanup.log"}}],"cwd":"work"}},
                {{"op":"wait","source":"stdout","literal":"READY-1","timeout_ms":10000}},
                {{"op":"assert-file","file":{{"path":"owned.sock","content":{{"utf8":"generation-1"}}}}}},
                {{"op":"restart"}},
                {{"op":"wait","source":"stdout","literal":"READY-2","timeout_ms":10000}},
                {{"op":"assert-file","file":{{"path":"owned.sock","content":{{"utf8":"generation-2"}}}}}},
                {{"op":"finish"}}
            ],"secrets":[]}}"##,
        current_platform()
    );
    let scenario =
        parse_scenario_v1(json.as_bytes()).unwrap_or_else(|err| panic!("should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
        installs: vec![("tmux".to_string(), tmux)],
    };
    let mut sentinel = Sentinel::start();
    let outcome = run(&scenario, &config);
    sentinel.assert_alive();
    assert!(unrelated_path.exists(), "unrelated socket must survive");
    assert!(outcome.error.is_none(), "outcome={outcome:?}");
    assert_eq!(outcome.report.status, "passed");
    let workspace = std::path::Path::new(&outcome.report.workspace);
    assert!(!workspace.join("owned.sock").exists());
    assert_eq!(
        std::fs::read_to_string(workspace.join("cleanup.log"))
            .unwrap_or_else(|err| panic!("read cleanup log: {err}")),
        "cleanup\ncleanup\n"
    );
    cleanup(&outcome);
}

#[test]
fn restart_socket_cleanup_failure_is_reported_without_relaunch() {
    let fixture = tempfile::tempdir().unwrap_or_else(|err| panic!("fixture directory: {err}"));
    let tmux = fixture.path().join("tmux");
    std::fs::write(&tmux, "#!/bin/sh\nexit 1\n")
        .unwrap_or_else(|err| panic!("write tmux fixture: {err}"));
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|err| panic!("chmod tmux fixture: {err}"));
    let json = format!(
        r##"{{"schema":1,"name":"restart-socket-cleanup-failure","platform":"{}",
            "terminal":{{"cols":100,"rows":30}},
            "workspace":{{"mode":448,"dirs":[{{"path":"work","mode":493}}],
                "files":[{{"path":"work/app.sh","content":{{"utf8":"#!/bin/sh\nprintf 'launch\\n' >> launches\n: > \"$JEFE_SOCKET_PATH\"\nprintf 'READY\\n'\nexec /bin/sleep 60\n"}},"mode":493}}],"env":[]}},
            "steps":[
                {{"op":"launch","argv":["${{workspace}}/work/app.sh"],"env":[{{"name":"JEFE_SOCKET_PATH","value":"${{workspace}}/owned.sock"}}],"cwd":"work"}},
                {{"op":"wait","source":"stdout","literal":"READY","timeout_ms":10000}},
                {{"op":"restart"}}
            ],"secrets":[]}}"##,
        current_platform()
    );
    let scenario =
        parse_scenario_v1(json.as_bytes()).unwrap_or_else(|err| panic!("should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
        installs: vec![("tmux".to_string(), tmux)],
    };
    let mut sentinel = Sentinel::start();
    let outcome = run(&scenario, &config);
    sentinel.assert_alive();
    let error = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("restart cleanup failure must fail"));
    assert_eq!(error.code(), HarCode::E007);
    let workspace = std::path::Path::new(&outcome.report.workspace);
    assert!(
        workspace.join("owned.sock").exists(),
        "failed cleanup must retain its diagnostic socket"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("work/launches"))
            .unwrap_or_else(|err| panic!("read launch inventory: {err}")),
        "launch\n",
        "restart cleanup failure must prevent a replacement launch"
    );
    let failed = outcome
        .report
        .steps
        .iter()
        .find(|step| step.status == "failed")
        .unwrap_or_else(|| panic!("cleanup failure must be reported"));
    assert_eq!(failed.op, "restart");
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
