//! Contract tests for issue #545 — the `windows_native` job must produce real,
//! complete Windows signal on every run.
//!
//! These read `.github/workflows/ci.yml` as text, matching the established
//! `read_repo_text` pattern in `windows_support_contracts.rs` and
//! `release_workflow_contracts.rs`.
//!
//! The invariant under test: portable, platform-independent checks must never
//! sit in front of the native steps where a failure skips all Windows
//! validation. Removing *gating* is the goal; removing *signal* is not, so
//! these tests also assert every portable check is still enforced somewhere.

use std::path::{Path, PathBuf};

const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

/// A1/A2: no portable, platform-independent check may run inside the
/// `windows_native` job, because a failure there skips every native step.
///
/// Measured evidence from the issue: of 15 `windows_native` failures in 80
/// runs, 7 were portable checks, and in all 7 the native steps never ran.
#[test]
fn windows_native_job_runs_no_portable_gate_before_native_steps() {
    let job = windows_native_job();
    for forbidden in [
        "cargo fmt",
        "cargo xtask check clippy-allows",
        "cargo xtask check source-size",
        "cargo xtask check architecture",
    ] {
        assert!(
            !job.contains(forbidden),
            "windows_native must not run the portable check `{forbidden}`: a \
             failure there skips every native step, producing zero Windows \
             signal. It is already enforced on Ubuntu."
        );
    }
}

/// A2: removing gating must not remove signal. Every portable check dropped
/// from `windows_native` must still be enforced by its own dedicated job, so
/// it reports independently instead of masking Windows results.
#[test]
fn portable_checks_are_enforced_off_the_windows_native_job() {
    let workflow = read_repo_text(CI_WORKFLOW);
    for (job, command) in [
        ("fmt:", "cargo fmt --all --check"),
        ("clippy_allow_policy:", "cargo xtask check clippy-allows"),
        ("source_file_size:", "cargo xtask check source-size"),
        ("architecture_policy:", "cargo xtask check architecture"),
    ] {
        assert!(
            workflow.contains(job),
            "portable check job `{job}` must still exist in CI"
        );
        assert!(
            workflow.contains(command),
            "portable check `{command}` must still be enforced somewhere in CI"
        );
    }
}

/// A3: Windows clippy has genuine value (a `cfg(windows)` lint gap is not
/// visible on Ubuntu), so it is kept — but as an independently reported job,
/// never as a prefix to the native suite.
#[test]
fn windows_clippy_is_an_independent_job() {
    let workflow = read_repo_text(CI_WORKFLOW);
    assert!(
        workflow.contains("  windows_clippy:"),
        "Windows-specific clippy must run as its own job so a cfg(windows) \
         lint gap is still caught without gating the native suite"
    );
    let job = job_body(&workflow, "windows_clippy");
    assert!(
        job.contains("windows-latest"),
        "windows_clippy must run on a Windows runner to see cfg(windows) code"
    );
    assert!(
        job.contains("-D warnings"),
        "windows_clippy must keep warnings-as-errors"
    );
    let native = windows_native_job();
    assert!(
        !native.contains("needs:") || !native.contains("windows_clippy"),
        "windows_native must not depend on windows_clippy; a lint failure \
         must not skip native steps"
    );
}

/// A4: `RUST_TEST_THREADS: 1` made every concurrency race structurally
/// invisible on the least-tested platform. Neither the job-level environment
/// variable nor the equivalent command-line flag may reappear.
#[test]
fn windows_native_does_not_serialize_the_workspace_suite() {
    let workflow = read_repo_text(CI_WORKFLOW);
    assert!(
        !workflow.contains("RUST_TEST_THREADS"),
        "RUST_TEST_THREADS serializes the whole 2,700-test workspace and hides \
         the multi-agent races real users hit; isolate individual tests instead"
    );
    assert!(
        !workflow.contains("--test-threads=1"),
        "--test-threads=1 is the same workspace-wide serialization by another \
         name; isolate individual tests instead"
    );
}

/// A6: every psmux test namespace generator must derive uniqueness from the
/// process id and a process-wide atomic counter, never from a timestamp alone.
///
/// Measured on a real Windows host, a timestamp-only generator produced 7,635
/// duplicate namespaces across 16,000 concurrent calls, because the system
/// clock's resolution is far coarser than a nanosecond. Colliding namespaces
/// mean a shared psmux server, which is the actual defect that
/// `RUST_TEST_THREADS: 1` was papering over.
#[test]
fn psmux_test_namespaces_never_depend_on_a_timestamp_alone() {
    for file in [
        "tests/psmux_smoke.rs",
        "tests/psmux_smoke_mouse.rs",
        "tests/psmux_orphan_reap.rs",
        "tests/psmux_session_host.rs",
        "tests/psmux_attach.rs",
        "tests/psmux_server_loss.rs",
        "tests/psmux_parallel_isolation.rs",
    ] {
        let source = read_repo_text(file);
        let generator = namespace_generator_body(&source, file);
        assert!(
            generator.contains("std::process::id()"),
            "{file}: the psmux namespace generator must include the process \
             id so two test processes cannot share a psmux server"
        );
        assert!(
            generator.contains("fetch_add"),
            "{file}: the psmux namespace generator must include a \
             process-wide atomic counter; the Windows clock is too coarse for \
             a timestamp to be unique between concurrent threads"
        );
    }
}

/// Return the body of the namespace-generating function in one psmux test.
fn namespace_generator_body(source: &str, file: &str) -> String {
    let start = source
        .find("fn unique_name(")
        .or_else(|| source.find("fn unique_namespace("))
        .unwrap_or_else(|| panic!("{file} must define a unique_name/unique_namespace generator"));
    let tail = &source[start..];
    let end = tail
        .find(
            "
}",
        )
        .map_or(tail.len(), |offset| offset + 2);
    tail[..end].to_owned()
}

/// A8: a job that "succeeded" because everything after an early step was
/// skipped is not a pass. A completion gate must assert every native step
/// actually executed, and must be a distinct required check.
#[test]
fn windows_native_completion_gate_rejects_skipped_native_steps() {
    let workflow = read_repo_text(CI_WORKFLOW);
    assert!(
        workflow.contains("  windows_native_complete:"),
        "a completion gate job must distinguish green-because-skipped from \
         green-because-passed"
    );
    let gate = job_body(&workflow, "windows_native_complete");
    assert!(
        gate.contains("needs:") && gate.contains("windows_native"),
        "the completion gate must depend on windows_native"
    );
    assert!(
        gate.contains("if: always()"),
        "the completion gate must evaluate even when windows_native fails or \
         is skipped, otherwise it is skipped too and proves nothing"
    );
    assert!(
        gate.contains("skipped"),
        "the completion gate must explicitly reject a skipped windows_native \
         result"
    );
    assert!(
        gate.contains("success"),
        "the completion gate must require windows_native to have succeeded"
    );
}

/// A7: Windows-only paths had no coverage enforcement at all, because the
/// coverage gate runs Ubuntu-only and those modules are compiled out there.
#[test]
fn ci_enforces_windows_only_module_coverage_floors() {
    let workflow = read_repo_text(CI_WORKFLOW);
    assert!(
        workflow.contains("  windows_coverage:"),
        "a Windows coverage job must exist; the Ubuntu coverage gate cannot \
         observe modules that are compiled out on Ubuntu"
    );
    let job = job_body(&workflow, "windows_coverage");
    assert!(
        job.contains("windows-latest"),
        "the coverage floors must be measured on a Windows runner"
    );
    assert!(
        job.contains("cargo xtask coverage-windows"),
        "the Windows coverage job must run the per-module floor gate"
    );
    assert!(
        !job.contains("continue-on-error"),
        "the Windows coverage gate must be able to fail the build"
    );
}

/// A9: intermittent failures must be attributable rather than argued about,
/// which requires a scheduled run on main producing a retrievable record.
#[test]
fn ci_records_a_scheduled_main_flake_baseline() {
    let workflow = read_repo_text(CI_WORKFLOW);
    assert!(
        workflow.contains("schedule:") && workflow.contains("cron:"),
        "CI must run on a schedule so main has a flake baseline"
    );
    assert!(
        workflow.contains("flake-baseline"),
        "the scheduled run must publish a retrievable flake record artifact"
    );
}

/// The native steps this job exists to run. If the job stops running any of
/// them, the Windows signal is incomplete regardless of the reported colour.
#[test]
fn windows_native_still_runs_every_native_step() {
    let job = windows_native_job();
    for required in [
        "Install pinned psmux",
        "Verify psmux version",
        "Build locked all-feature workspace",
        "Run installer Pester harness",
        "Test locked all-feature workspace",
        "Run deterministic startup and quit TUI scenario",
        "Verify renderer viewport edges without full redraws",
        "Clean package lifecycle",
        "Run real psmux startup-quit against installed binary",
    ] {
        assert!(
            job.contains(required),
            "windows_native must still run the native step `{required}`"
        );
    }
}

/// Issue #542, V7. The orphan check recorded surviving psmux *sessions* into an
/// artifact and then `exit 0`, so the leak class this repository has reopened
/// four times could never turn CI red. Sessions are also the wrong unit: #515's
/// signature is a session that has already vanished from the inventory while
/// its `jefe-session-host.exe` and worker keep running.
#[test]
fn windows_native_fails_when_any_jefe_process_survives_the_suite() {
    let job = windows_native_job();
    let step = step_body(&job, "Assert no jefe process survives the suite");

    assert!(
        step.contains("if: always()"),
        "the survivor gate must run even when an earlier native step failed; \
         a failing suite is exactly when trees leak"
    );
    assert!(
        !step.contains("continue-on-error"),
        "the survivor gate must be able to fail the build; recording orphans \
         into an artifact is what let this defect class reopen four times"
    );
    assert!(
        step.contains("jefe-session-host"),
        "the gate must count surviving session-host processes, not only psmux \
         sessions: a leaked tree outlives the session that owned it"
    );
    assert!(
        step.contains("throw"),
        "the gate must throw on a surviving process instead of exiting 0"
    );
}

/// Return one `- name: <step>` block from a job body, up to the next step.
fn step_body(job: &str, step: &str) -> String {
    let header = format!("- name: {step}");
    let mut lines = job
        .lines()
        .skip_while(|line| line.trim_start() != header.as_str());
    let Some(first) = lines.next() else {
        panic!("step `{step}` is not present in the job");
    };
    let indent = first.len() - first.trim_start().len();
    let mut body = String::from(first);
    for line in lines {
        let starts_new_step = line.trim_start().starts_with("- name:")
            && line.len() - line.trim_start().len() <= indent;
        if starts_new_step {
            break;
        }
        body.push('\n');
        body.push_str(line);
    }
    body
}

/// Extract the `windows_native` job body from the CI workflow.
fn windows_native_job() -> String {
    let workflow = read_repo_text(CI_WORKFLOW);
    job_body(&workflow, "windows_native")
}

/// Return the YAML block for one top-level job: every line from the job key
/// until the next key at the same two-space indentation.
fn job_body(workflow: &str, job: &str) -> String {
    let header = format!("  {job}:");
    let mut lines = workflow.lines().skip_while(|line| *line != header);
    let Some(first) = lines.next() else {
        panic!("job `{job}` is not present in {CI_WORKFLOW}");
    };
    let mut body = String::from(first);
    for line in lines {
        let is_next_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-');
        if is_next_job {
            break;
        }
        body.push('\n');
        body.push_str(line);
    }
    body
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
