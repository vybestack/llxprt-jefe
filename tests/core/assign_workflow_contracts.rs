//! Assign Issue workflow contract tests (issue #499).
//!
//! These tests verify the `/assign` self-assignment automation ported from
//! `vybestack/llxprt-code` (issue #499) is structurally wired into this
//! repository. They read the workflows, scripts, and CONTRIBUTING.md as text
//! (mirroring `ocr_workflow_contracts` and `pr_review_workflow_contracts`)
//! and assert that each acceptance criterion from the issue plan is present.
//!
//! The automation is bash + `gh` + `jq` (no Rust product code), so behavioral
//! coverage is structural: CI fails mechanically if a workflow trigger,
//! script reference, jefe-scoped marker, repo guard, eligibility helper, cap
//! constant, election/rollback logic, or documentation section regresses.

use std::path::{Path, PathBuf};

const ASSIGN_WORKFLOW_PATH: &str = ".github/workflows/assign.yml";
const STALE_CLEANUP_WORKFLOW_PATH: &str = ".github/workflows/assign-stale-cleanup.yml";
const ASSIGN_ISSUE_SCRIPT_PATH: &str = ".github/scripts/assign-issue.sh";
const RECORD_HISTORY_SCRIPT_PATH: &str = ".github/scripts/record-assignment-history.sh";
const UNASSIGN_STALE_SCRIPT_PATH: &str = ".github/scripts/unassign-stale-issues.sh";
const ASSIGN_CONSTANTS_SCRIPT_PATH: &str = ".github/scripts/assign-constants.sh";
const CONTRIBUTING_PATH: &str = "CONTRIBUTING.md";

/// The jefe-scoped feedback marker, mirroring the `<!-- jefe-ocr-review -->`
/// convention used in `ocr-review.yml`.
const EXPECTED_MARKER: &str = "<!-- jefe-assign-feedback -->";

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}

fn read_file(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn read_workflow(workflow_path: &str) -> String {
    read_file(repo_path(workflow_path))
}

fn read_script(script_path: &str) -> String {
    read_file(repo_path(script_path))
}

/// Extract the body of a single named workflow job, starting from the
/// `  <job_id>:` line until the next top-level job key. This isolates
/// assertions so comments or content from unrelated jobs cannot satisfy a
/// contract check. Job keys live at 2-space indentation under `jobs:`.
fn job_body(content: &str, job_id: &str) -> String {
    let needle = format!("  {job_id}:");
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == needle)
        .unwrap_or_else(|| panic!("job '{job_id}' not found in workflow"));

    let mut body = String::new();
    body.push_str(lines[start]);
    body.push('\n');

    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // Stop at the next job header (2-space indented `key:` that is not
        // the current job_id) or at a top-level key (0 indentation).
        if indent <= 2 && trimmed.ends_with(':') && !trimmed.starts_with('#') {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

// ---------------------------------------------------------------------------
// A1: assign.yml triggers on exact /assign for open issues, excluding PRs/bots
// ---------------------------------------------------------------------------

#[test]
fn assign_workflow_triggers_on_issue_comment_created() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    assert!(
        content.contains("issue_comment:") && content.contains("- created"),
        "assign.yml must trigger on issue_comment created events"
    );
}

#[test]
fn assign_job_matches_exact_assign_command() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    let job = job_body(&content, "assign");
    // Exact /assign only — support LF, CRLF, and trailing tab via toJSON
    // equality so /assign foo never matches.
    assert!(
        job.contains("github.event.comment.body == '/assign'"),
        "assign job must match exact '/assign' (A1)"
    );
    assert!(
        job.contains("toJSON(github.event.comment.body) == '\"/assign\\n\"'"),
        "assign job must support trailing LF via toJSON (A1)"
    );
    assert!(
        job.contains("toJSON(github.event.comment.body) == '\"/assign\\r\\n\"'"),
        "assign job must support trailing CRLF via toJSON (A1)"
    );
    assert!(
        job.contains("toJSON(github.event.comment.body) == '\"/assign\\t\"'"),
        "assign job must support trailing tab via toJSON (A1)"
    );
}

#[test]
fn assign_job_excludes_pr_comments_and_bots() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    let job = job_body(&content, "assign");
    assert!(
        job.contains("github.event.issue.pull_request == null"),
        "assign job must ignore PR comments (A1)"
    );
    assert!(
        job.contains("github.event.issue.state == 'open'"),
        "assign job must only run on open issues (A1)"
    );
    assert!(
        job.contains("github.event.comment.user.type != 'Bot'"),
        "assign job must ignore bot accounts (A1)"
    );
}

#[test]
fn assign_job_calls_assign_issue_script() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    let job = job_body(&content, "assign");
    assert!(
        job.contains(".github/scripts/assign-issue.sh"),
        "assign job must invoke .github/scripts/assign-issue.sh (A1-A4)"
    );
}

// ---------------------------------------------------------------------------
// A3: cap enforcement (MAX_ASSIGNMENTS=3)
// ---------------------------------------------------------------------------

#[test]
fn assign_issue_script_enforces_three_issue_cap() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains("MAX_ASSIGNMENTS=3"),
        "assign-issue.sh must enforce the 3-issue cap (A3)"
    );
    assert!(
        content.contains("get_open_assigned_count"),
        "assign-issue.sh must have an open-assigned-count helper for cap checks (A3)"
    );
}

// ---------------------------------------------------------------------------
// A2: eligibility helpers (merged PR or prior assignment)
// ---------------------------------------------------------------------------

#[test]
fn assign_issue_script_has_merged_pr_eligibility() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains("get_merged_pr_count"),
        "assign-issue.sh must check merged PR count for eligibility (A2)"
    );
}

#[test]
fn assign_issue_script_has_historical_assignment_eligibility() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains("has_historical_assignment"),
        "assign-issue.sh must check prior assignment history for eligibility (A2)"
    );
}

// ---------------------------------------------------------------------------
// A4: election + verified rollback logic
// ---------------------------------------------------------------------------

#[test]
fn assign_issue_script_has_winner_election() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains("elect_winner"),
        "assign-issue.sh must have a deterministic winner election (A4)"
    );
    assert!(
        content.contains("run_election"),
        "assign-issue.sh must run the election (A4)"
    );
}

#[test]
fn assign_issue_script_has_verified_rollback() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains("verified_rollback_and_fail"),
        "assign-issue.sh must have verified rollback on contention (A4)"
    );
    assert!(
        content.contains("rollback_this_run"),
        "assign-issue.sh must roll back this run's mutations (A4)"
    );
}

// ---------------------------------------------------------------------------
// A7: jefe-scoped feedback marker
// ---------------------------------------------------------------------------

#[test]
fn assign_issue_script_uses_jefe_feedback_marker() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        content.contains(EXPECTED_MARKER),
        "assign-issue.sh must use the jefe-scoped feedback marker '{EXPECTED_MARKER}' (A7)"
    );
}

#[test]
fn assign_issue_script_does_not_use_llxprt_marker() {
    let content = read_script(ASSIGN_ISSUE_SCRIPT_PATH);
    assert!(
        !content.contains("llxprt-assign-feedback"),
        "assign-issue.sh must not carry the upstream llxprt-code marker (A7)"
    );
}

// ---------------------------------------------------------------------------
// A5: record-history job + record-assignment-history.sh
// ---------------------------------------------------------------------------

#[test]
fn assign_workflow_triggers_on_issues_assigned() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    assert!(
        content.contains("issues:") && content.contains("- assigned"),
        "assign.yml must trigger on issues assigned events (A5)"
    );
}

#[test]
fn record_history_job_calls_record_script() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    let job = job_body(&content, "record-history");
    assert!(
        job.contains(".github/scripts/record-assignment-history.sh"),
        "record-history job must invoke .github/scripts/record-assignment-history.sh (A5)"
    );
}

// ---------------------------------------------------------------------------
// A6: stale cleanup workflow + repo guard + script
// ---------------------------------------------------------------------------

#[test]
fn stale_cleanup_workflow_exists_and_runs_script() {
    let content = read_workflow(STALE_CLEANUP_WORKFLOW_PATH);
    assert!(
        content.contains(".github/scripts/unassign-stale-issues.sh"),
        "assign-stale-cleanup.yml must invoke .github/scripts/unassign-stale-issues.sh (A6)"
    );
}

#[test]
fn stale_cleanup_workflow_guards_on_jefe_repo() {
    let content = read_workflow(STALE_CLEANUP_WORKFLOW_PATH);
    // The scheduled-run repo guard must restrict to the canonical jefe repo,
    // not the upstream llxprt-code repo and not forks.
    assert!(
        content.contains("github.repository == 'vybestack/llxprt-jefe'"),
        "assign-stale-cleanup.yml must guard scheduled runs to vybestack/llxprt-jefe (A6)"
    );
    assert!(
        !content.contains("github.repository == 'vybestack/llxprt-code'"),
        "assign-stale-cleanup.yml must not carry the upstream llxprt-code repo guard (A6)"
    );
}

#[test]
fn stale_cleanup_workflow_has_schedule_and_dispatch() {
    let content = read_workflow(STALE_CLEANUP_WORKFLOW_PATH);
    assert!(
        content.contains("schedule:") && content.contains("cron:"),
        "assign-stale-cleanup.yml must have a schedule trigger (A6)"
    );
    assert!(
        content.contains("workflow_dispatch:"),
        "assign-stale-cleanup.yml must have a workflow_dispatch trigger (A6)"
    );
}

#[test]
fn unassign_stale_script_preserves_exempt_and_coassignees() {
    let content = read_script(UNASSIGN_STALE_SCRIPT_PATH);
    assert!(
        content.contains("EXEMPT_LOGIN='acoliver'"),
        "unassign-stale-issues.sh must exempt acoliver from cleanup (A6)"
    );
    assert!(
        content.contains("STALE_DAYS=14"),
        "unassign-stale-issues.sh must use a 14-day stale threshold (A6)"
    );
    // Targeted DELETE preserves co-assignees; whole-array PATCH would not.
    assert!(
        content.contains("--method DELETE") && content.contains("assignees[]"),
        "unassign-stale-issues.sh must use targeted DELETE for assignee removal (A6)"
    );
}

// ---------------------------------------------------------------------------
// Shared constants + script existence
// ---------------------------------------------------------------------------

#[test]
fn assign_constants_script_has_history_label_constants() {
    let content = read_script(ASSIGN_CONSTANTS_SCRIPT_PATH);
    assert!(
        content.contains("HISTORY_PREFIX='asnhist--'"),
        "assign-constants.sh must define the asnhist-- history label prefix"
    );
    assert!(
        content.contains("validate_github_login"),
        "assign-constants.sh must define the login validator"
    );
}

#[test]
fn record_history_script_validates_label_definition() {
    let content = read_script(RECORD_HISTORY_SCRIPT_PATH);
    assert!(
        content.contains("validate_history_label"),
        "record-assignment-history.sh must validate the history label definition (A5)"
    );
    assert!(
        content.contains("assign-constants.sh"),
        "record-assignment-history.sh must source the shared constants"
    );
}

#[test]
fn all_automation_scripts_exist() {
    for script in [
        ASSIGN_ISSUE_SCRIPT_PATH,
        RECORD_HISTORY_SCRIPT_PATH,
        UNASSIGN_STALE_SCRIPT_PATH,
        ASSIGN_CONSTANTS_SCRIPT_PATH,
    ] {
        let path = repo_path(script);
        assert!(
            path.is_file(),
            "automation script {script} must exist in the repository",
        );
    }
}

// ---------------------------------------------------------------------------
// A8: CONTRIBUTING.md documents the /assign convention
// ---------------------------------------------------------------------------

#[test]
fn contributing_documents_self_assigning_issues() {
    let content = read_file(repo_path(CONTRIBUTING_PATH));
    assert!(
        content.contains("## Self Assigning Issues"),
        "CONTRIBUTING.md must have a 'Self Assigning Issues' section (A8)"
    );
    assert!(
        content.contains("/assign"),
        "CONTRIBUTING.md must document the /assign command (A8)"
    );
    assert!(
        content.contains("auto-assigned"),
        "CONTRIBUTING.md must document the auto-assigned label (A8)"
    );
}

// ---------------------------------------------------------------------------
// Permissions + concurrency guards (security/anti-spam hardening)
// ---------------------------------------------------------------------------

#[test]
fn assign_workflow_has_safe_permissions() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    // issues: write is required for assignment; pull-requests: read for the
    // merged-PR eligibility check. contents: read for checkout.
    assert!(
        content.contains("issues: write"),
        "assign.yml must have issues: write permission"
    );
    assert!(
        content.contains("pull-requests: read"),
        "assign.yml must have pull-requests: read permission"
    );
}

#[test]
fn assign_job_has_concurrency_group_per_commenter_and_issue() {
    let content = read_workflow(ASSIGN_WORKFLOW_PATH);
    let job = job_body(&content, "assign");
    assert!(
        job.contains("concurrency:"),
        "assign job must have a concurrency group (anti-spam hardening)"
    );
    assert!(
        job.contains("github.event.comment.user.id"),
        "assign job concurrency must be grouped per commenter user id"
    );
    assert!(
        job.contains("github.event.issue.number"),
        "assign job concurrency must be grouped per issue number"
    );
}
