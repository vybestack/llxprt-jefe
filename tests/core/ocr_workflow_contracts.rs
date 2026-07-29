//! OCR review workflow contract tests (issues #310 and #500).
//!
//! These tests verify the operational controls required by issues #310 and #500 are
//! present in `.github/workflows/ocr-review.yml` without weakening any quality
//! rules or adding suppressions. They read the workflow as text (the same
//! approach used by `tmux_harness_docs_contracts`) and assert that each
//! acceptance criterion is structurally wired. Assertions are scoped to
//! specific named step bodies so comments or unrelated steps cannot satisfy
//! a contract check.

use std::path::{Path, PathBuf};

const WORKFLOW_PATH: &str = ".github/workflows/ocr-review.yml";

fn read_workflow() -> String {
    let path = repo_path(WORKFLOW_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}

/// Extract the body of a single named workflow step, starting from the
/// `- name: <step_name>` line until the next top-level `- name:` or
/// step-level key at the same indentation. This isolates assertions so
/// comments or content from unrelated steps cannot satisfy a contract.
fn step_body(content: &str, step_name: &str) -> String {
    let needle = format!("- name: {step_name}");
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == needle)
        .unwrap_or_else(|| panic!("step '{step_name}' not found in workflow"));

    // The first line after `- name:` determines the indentation of step
    // child keys (typically 8 spaces for steps under jobs.<id>.steps).
    let step_indent = lines
        .get(start + 1)
        .map_or(8, |l| l.len() - l.trim_start().len());

    let mut body = String::new();
    body.push_str(lines[start]);
    body.push('\n');

    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // Stop at the next step (`- name:` at the same indentation as the
        // enclosing steps list) or at a key at a lower indentation (like
        // the job-level `- name: Notify...` or a new job header).
        if indent < step_indent && !trimmed.starts_with('#') {
            break;
        }
        if indent == step_indent && trimmed.starts_with("- name:") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

// ---------------------------------------------------------------------------
// Criterion 1: single exact OCR version source
// ---------------------------------------------------------------------------

#[test]
fn ocr_version_is_declared_exactly_once() {
    let content = read_workflow();
    // Count non-comment lines declaring OCR_VERSION — exactly one.
    let declarations: Vec<&str> = content
        .lines()
        .filter(|l| {
            let trimmed = l.trim();
            trimmed.starts_with("OCR_VERSION:") && !trimmed.starts_with('#')
        })
        .collect();
    assert_eq!(
        declarations.len(),
        1,
        "OCR_VERSION must be declared exactly once as the single source of truth"
    );
    let raw_value = declarations[0]
        .trim()
        .strip_prefix("OCR_VERSION:")
        .unwrap_or_else(|| panic!("OCR_VERSION line was malformed"))
        .trim();
    let unquoted = raw_value.trim_matches(|ch| ch == '"' || ch == '\'');
    assert_eq!(
        unquoted, "1.7.9",
        "OCR_VERSION must be pinned to the reviewed exact version 1.7.9"
    );
}

#[test]
fn ocr_install_references_version_variable_not_literal() {
    let content = read_workflow();
    assert!(
        content.contains("\"@alibaba-group/open-code-review@${OCR_VERSION}\""),
        "Install must reference the single-source OCR_VERSION variable, not a hard-coded literal"
    );
}

#[test]
fn ocr_install_literal_version_is_absent() {
    // The literal pinned version must NOT appear in the install command.
    // It may only appear in the OCR_VERSION declaration. Checking for zero
    // occurrences of the full install literal guarantees the install path
    // uses the variable exclusively.
    let content = read_workflow();
    let install_literal_count = content
        .lines()
        .filter(|l| l.contains("@alibaba-group/open-code-review@1.7.9"))
        .count();
    assert_eq!(
        install_literal_count, 0,
        "The literal @alibaba-group/open-code-review@1.7.9 must not appear in the workflow; the install command must reference ${{OCR_VERSION}}"
    );
}

#[test]
fn ocr_npm_cache_uses_version_variable() {
    let content = read_workflow();
    // The npm download cache key must reference OCR_VERSION so a version
    // bump does not leave a stale cache pointing at the old version.
    assert!(
        content.contains("npm-ocr-") && content.contains("${{ env.OCR_VERSION }}"),
        "npm cache key must reference ${{ env.OCR_VERSION }} so cache identity tracks the single version source"
    );
}

// ---------------------------------------------------------------------------
// Criterion 2: bounded connectivity preflight
// ---------------------------------------------------------------------------

#[test]
fn ocr_has_bounded_connectivity_preflight() {
    let content = read_workflow();
    // Scope to the connectivity step body so the assertions cannot be
    // satisfied by comments or other steps mentioning timeouts.
    let preflight = content
        .lines()
        .find(|l| l.contains("llm test"))
        .unwrap_or_else(|| {
            panic!("Workflow must run ocr llm test as a bounded connectivity preflight")
        });
    let _ = preflight; // verify presence
    let step = step_body(&content, "Validate OCR LLM connectivity");
    assert!(
        step.contains("llm test"),
        "Connectivity step must run 'ocr llm test'"
    );
    assert!(
        step.contains("timeout 120s"),
        "Connectivity preflight must be bounded by an explicit timeout wrapper"
    );
    // Exit code 124 is the standard GNU coreutils timeout kill code.
    assert!(
        step.contains("124"),
        "Connectivity preflight must distinguish timeout (exit 124) from other failures"
    );
}

// ---------------------------------------------------------------------------
// Criterion 3: configurable provider concurrency
// ---------------------------------------------------------------------------

#[test]
fn ocr_review_uses_provider_concurrency_budget() {
    let content = read_workflow();
    assert!(
        content.contains("--concurrency 2"),
        "OCR review must cap provider contention with --concurrency 2"
    );
}

// ---------------------------------------------------------------------------
// Criterion 4: typed provider-failure classification
// ---------------------------------------------------------------------------

#[test]
fn ocr_review_classifies_rate_limit_and_overloaded() {
    let content = read_workflow();
    for signal in ["http 429", "rate limit"] {
        assert!(
            content.contains(signal),
            "OCR review must classify rate-limit signal: missing {signal:?}"
        );
    }
    assert!(
        content.contains("529"),
        "OCR review must distinguish HTTP 529 (provider overloaded) from generic failures"
    );
}

#[test]
fn ocr_review_classifies_all_file_and_auth_failures() {
    let content = read_workflow();
    // The all-file grep pattern and the reason text must both be present.
    assert!(
        content.contains("all [0-9]+ file review"),
        "OCR review must grep for wholesale per-file review failures"
    );
    assert!(
        content.contains("provider/config/auth"),
        "OCR review must classify wholesale failure as a provider/config/auth issue"
    );
}

#[test]
fn ocr_review_classifies_timeout_distinctly() {
    // Assert the specific timeout classification branch exists (the grep
    // pattern + the reason text passed to mark_infrastructure_failure),
    // not just a generic "timed out" comment.
    let content = read_workflow();
    assert!(
        content.contains("timed out|timeout"),
        "OCR review must classify timeout distinctly via a grep pattern"
    );
    assert!(
        content.contains("OCR review timed out"),
        "OCR review must map timeout stderr to a distinct timeout reason classification"
    );
}

// ---------------------------------------------------------------------------
// Criterion 5: fail-closed redaction (placeholder before redaction)
// ---------------------------------------------------------------------------

#[test]
fn ocr_redaction_destroys_original_before_redaction() {
    let content = read_workflow();
    // Scope to the redaction step so the assertions bind to the actual
    // redaction loop, not comments in other steps.
    let step = step_body(&content, "Redact OCR diagnostic artifacts");

    // The fail-closed placeholder must use the specific format.
    assert!(
        step.contains("[redaction unavailable for"),
        "Redaction step must write a safe placeholder before attempting redaction"
    );
    // The placeholder write must precede the redacted-content write.
    let placeholder_pos = step
        .find("[redaction unavailable for")
        .unwrap_or_else(|| panic!("placeholder text not found in redaction step"));
    let redact_pos = step
        .find("redact(raw)")
        .unwrap_or_else(|| panic!("redact(raw) call not found in redaction step"));
    assert!(
        placeholder_pos < redact_pos,
        "Placeholder write must precede the redacted-content write so a write error cannot leak secrets"
    );
    // When the placeholder write itself fails, the file must be removed so
    // the original unredacted content cannot be uploaded.
    assert!(
        step.contains("rmSync"),
        "Redaction must remove the file if the placeholder write fails, preventing upload of unredacted content"
    );
}

#[test]
fn ocr_upload_skipped_on_redaction_failure() {
    let content = read_workflow();
    // The upload step must be conditioned on the redaction step succeeding.
    assert!(
        content.contains("steps.redact-ocr-artifacts.outcome == 'success'"),
        "Upload step must be skipped when redaction fails (id: redact-ocr-artifacts)"
    );
}

// ---------------------------------------------------------------------------
// Criterion 6: retry only reads and idempotent operations
// ---------------------------------------------------------------------------

#[test]
fn ocr_notification_retries_reads_not_writes() {
    let content = read_workflow();
    // gh issue list (search) is a read — it may be retried safely.
    assert!(
        content.contains("retry_gh gh issue list"),
        "Notification must retry read operations (gh issue list search)"
    );
    // gh issue create and gh issue comment are non-idempotent writes — they
    // must NOT be wrapped in retry_gh.
    assert!(
        !content.contains("retry_gh gh issue create"),
        "Notification must NOT retry non-idempotent writes (gh issue create)"
    );
    assert!(
        !content.contains("retry_gh gh issue comment"),
        "Notification must NOT retry non-idempotent writes (gh issue comment)"
    );
}

#[test]
fn ocr_notification_reconciles_ambiguous_writes() {
    let content = read_workflow();
    // Exactly three gh issue list calls must exist:
    //   1. converge_tracking_issues — duplicate convergence sweep
    //   2. initial lookup before creating (sort:created-asc)
    //   3. pre-create recheck to narrow the race window (sort:created-asc)
    // An exact count catches both accidental removal of a reconciliation
    // search and addition of an unnecessary duplicate.
    let recheck_count = content.matches("gh issue list").count();
    assert_eq!(
        recheck_count, 3,
        "Notification must have exactly 3 gh issue list calls for reconciliation (found {recheck_count})"
    );
}

// ---------------------------------------------------------------------------
// Criterion 7: label-less fallback only for verified missing-label response
// ---------------------------------------------------------------------------

#[test]
fn ocr_label_less_fallback_requires_422_and_label_evidence() {
    // The label-less fallback must require BOTH a 422 status code AND label
    // evidence in the error, not match on either independently.
    let content = read_workflow();
    assert!(
        content.contains("422") && content.contains("label|ci/cd"),
        "Label-less fallback must check for both HTTP 422 and label/ci/cd evidence"
    );
    // The fallback must use a compound condition (&&), not a single grep.
    assert!(
        content.contains("grep -Eq '(^|[^0-9])422([^0-9]|$)'")
            && content.contains("grep -Eqi \"label|ci/cd\""),
        "Label-less fallback must require both 422 and label evidence via a compound condition"
    );
}

// ---------------------------------------------------------------------------
// Criterion 8: serialize tracking notifications and converge duplicates
// ---------------------------------------------------------------------------

#[test]
fn ocr_notification_converges_duplicate_tracking_issues() {
    let content = read_workflow();
    assert!(
        content.contains("cancel-in-progress: false"),
        "Tracking notification job must serialize (cancel-in-progress: false)"
    );
    // The convergence function must exist by name and close duplicates.
    assert!(
        content.contains("converge_tracking_issues"),
        "Notification must define a converge_tracking_issues function"
    );
    assert!(
        content.contains("gh issue close"),
        "converge_tracking_issues must close duplicate tracking issues"
    );
    // The convergence must be called on all notification paths (comment and create).
    let converge_calls = content.matches("converge_tracking_issues || true").count();
    assert!(
        converge_calls >= 2,
        "converge_tracking_issues must be called on both the comment and create paths (found {converge_calls} call sites)"
    );
}

// ---------------------------------------------------------------------------
// Criterion 9: deduplicate exact candidates before batch posting
// ---------------------------------------------------------------------------

#[test]
fn ocr_deduplicates_findings_before_posting() {
    let content = read_workflow();
    assert!(
        content.contains("findingIdentityKey") && content.contains("dedupedFindings"),
        "Post-OCR posting must deduplicate exact candidates from the current result before batch posting"
    );
    // The dedup key must normalize reversed ranges (startLine > endLine).
    assert!(
        content.contains("startLine > endLine")
            || content.contains("[startLine, endLine] = [endLine, startLine]"),
        "Dedup key must normalize reversed line ranges so 10-5 and 5-10 collapse to the same key"
    );
}

// ---------------------------------------------------------------------------
// Issue #464: manifest builder has no undeclared action-runtime dependencies
// ---------------------------------------------------------------------------

#[test]
fn ocr_manifest_builder_uses_dependency_free_workflow_commands() {
    let content = read_workflow();
    let step = step_body(&content, "Build OCR reproducibility manifests");

    assert!(
        !step.contains("require('@actions/core')") && !step.contains("require(\"@actions/core\")"),
        "Plain-shell manifest Node must not import @actions/core, which is not installed in the repository module path"
    );
    assert!(
        step.contains("::warning::"),
        "Manifest diagnostics must retain GitHub Actions warning semantics without @actions/core"
    );
}

/// Return the 0-based line index of the `- name: <step_name>` header line, or
/// `None` when the step is absent. Used to assert workflow step ordering.
fn step_line(content: &str, step_name: &str) -> Option<usize> {
    let needle = format!("- name: {step_name}");
    content.lines().position(|l| l.trim() == needle)
}

// ---------------------------------------------------------------------------
// Issue #464: true pre-run reproducibility manifest (structural ordering)
// ---------------------------------------------------------------------------

#[test]
fn pre_run_manifest_step_exists_before_run_step() {
    let content = read_workflow();
    let pre_line = step_line(&content, "Write pre-run OCR reproducibility manifest");
    let run_line = step_line(&content, "Run OpenCodeReview");
    let pre = pre_line.unwrap_or_else(|| {
        panic!("Workflow must define a 'Write pre-run OCR reproducibility manifest' step")
    });
    let run = run_line.unwrap_or_else(|| panic!("'Run OpenCodeReview' step not found"));
    assert!(
        pre < run,
        "The pre-run manifest step must run BEFORE 'Run OpenCodeReview' so manifest.pre.json is a true launch-time snapshot (pre line {pre} must precede run line {run})"
    );
}

#[test]
fn pre_run_manifest_step_captures_trusted_base() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    // The trusted base is the checked-out base (HEAD after trusted checkout),
    // distinct from the merge-base scope. Both the HEAD and the base branch
    // name must be recorded.
    assert!(
        step.contains("trusted_base"),
        "Pre-run manifest must record the trusted_base block (checked-out HEAD + branch, distinct from the reviewed merge-base)"
    );
    assert!(
        step.contains("rev-parse HEAD") || step.contains("git rev-parse HEAD"),
        "Pre-run manifest must capture the trusted checkout HEAD via git rev-parse"
    );
    assert!(
        step.contains("base_ref") || step.contains("BASE_REF"),
        "Pre-run manifest must record the trusted base branch name"
    );
}

#[test]
fn pre_run_manifest_step_captures_worktree_state() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    // Worktree state: clean flag + diff hashes for staged/unstaged/untracked.
    assert!(
        step.contains("worktree"),
        "Pre-run manifest must record worktree state"
    );
    assert!(
        step.contains("staged_diff_sha256"),
        "Pre-run manifest must record the worktree clean flag and diff hashes"
    );
}

#[test]
fn pre_run_manifest_step_captures_control_and_scope_args() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    assert!(
        step.contains("control_args") || step.contains("controlArgs"),
        "Pre-run manifest must record the fixed OCR control argument vector"
    );
    assert!(
        step.contains("--audience") && step.contains("--concurrency") && step.contains("--timeout"),
        "Pre-run manifest must record the fixed control args (--audience, --concurrency, --timeout)"
    );
    assert!(
        step.contains("scope_args") || step.contains("scopeArgs"),
        "Pre-run manifest must record the exact scope argument vector"
    );
}

#[test]
fn pre_run_manifest_step_captures_rule_hash() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    assert!(
        step.contains("rule") && step.contains("sha256"),
        "Pre-run manifest must record the sha256 of the OCR rule.json used by CI"
    );
}

#[test]
fn pre_run_manifest_step_records_comparison_eligibility() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    assert!(
        step.contains("comparison_eligible") || step.contains("comparisonEligible"),
        "Pre-run manifest must record an explicit comparison_eligible field so eligibility is machine-supported"
    );
}

#[test]
fn pre_run_manifest_step_is_dependency_free() {
    let content = read_workflow();
    let step = step_body(&content, "Write pre-run OCR reproducibility manifest");
    assert!(
        !step.contains("require('@actions/core')") && !step.contains("require(\"@actions/core\")"),
        "Pre-run manifest Node must not import @actions/core (mirrors the post-step contract)"
    );
}

// ---------------------------------------------------------------------------
// Issue #464: post-manifest completeness and pre-snapshot preservation
// ---------------------------------------------------------------------------

#[test]
fn post_manifest_step_does_not_overwrite_pre_snapshot() {
    let content = read_workflow();
    let step = step_body(&content, "Build OCR reproducibility manifests");
    // The post step must write manifest.post.json only. It must NOT rewrite
    // manifest.pre.json, which is now a true pre-run snapshot written earlier.
    assert!(
        !step.contains("writeFileSync('manifest.pre.json'")
            && !step.contains("writeFileSync(\"manifest.pre.json\""),
        "Post manifest step must not overwrite the pre-run manifest.pre.json snapshot"
    );
}

#[test]
fn post_manifest_step_carries_run_id_and_parse_error_and_pre_artifact() {
    let content = read_workflow();
    let step = step_body(&content, "Build OCR reproducibility manifests");
    // The post manifest must record run_id, parse_error, and include
    // manifest.pre.json in its artifacts map.
    assert!(step.contains("run_id"), "Post manifest must record run_id");
    assert!(
        step.contains("parse_error"),
        "Post manifest must record a parse_error field"
    );
    assert!(
        step.contains("'manifest.pre.json'") || step.contains("\"manifest.pre.json\""),
        "Post manifest artifacts map must include the manifest.pre.json hash"
    );
}

// ---------------------------------------------------------------------------
// Criterion 10: preserve existing protections
// ---------------------------------------------------------------------------

#[test]
fn ocr_preserves_fork_safety_and_same_head_filter() {
    let content = read_workflow();
    assert!(
        content.contains("pull_request_target"),
        "Workflow must preserve pull_request_target for fork safety"
    );
    assert!(
        content.contains("persist-credentials: false"),
        "Workflow must persist fork-safety checkout (persist-credentials: false)"
    );
    assert!(
        content.contains("MARKER"),
        "Workflow must preserve the sticky marker for same-head deduplication"
    );
    assert!(
        content.contains("cancel-in-progress: true"),
        "code-review job must preserve per-PR cancellation"
    );
}

#[test]
fn ocr_preserves_rust_test_scope_guard() {
    let content = read_workflow();
    assert!(
        content.contains("Verify review scope includes changed tests"),
        "Workflow must preserve the Rust test-scope guard step"
    );
    assert!(
        content.contains("Will review"),
        "Workflow must preserve the 'Will review' scope verification"
    );
}

// ---------------------------------------------------------------------------
// Issue #500: bound automatic post-open OCR reviews
// ---------------------------------------------------------------------------

#[test]
fn ocr_budget_gate_runs_before_checkout_and_gates_later_review_steps() {
    let content = read_workflow();
    let resolution_step = step_body(&content, "Resolve PR context");
    assert!(
        resolution_step.contains("dispatchInput !== String(number)")
            && resolution_step.contains("canonical positive decimal form"),
        "Workflow dispatch must reject noncanonical PR spellings before any budget-state access"
    );
    let gate_name = "Check automatic OCR review budget";
    let gate_position = content
        .find(&format!("- name: {gate_name}"))
        .unwrap_or_else(|| panic!("automatic OCR budget gate not found"));
    let checkout_position = content
        .find("- name: Checkout trusted base")
        .unwrap_or_else(|| panic!("trusted checkout step not found"));
    let install_position = content
        .find("- name: Install OpenCodeReview")
        .unwrap_or_else(|| panic!("OCR install step not found"));
    assert!(
        gate_position < checkout_position && gate_position < install_position,
        "Automatic OCR budget must be decided before checkout and OCR installation"
    );

    let code_review_start = content
        .find("  code-review:")
        .unwrap_or_else(|| panic!("code-review job not found"));
    let code_review_tail = &content[code_review_start..];
    let code_review_end = code_review_tail
        .find("\n  notify-ocr-infrastructure-failure:")
        .unwrap_or_else(|| panic!("code-review job end not found"));
    let step_names = code_review_tail[..code_review_end]
        .lines()
        .filter_map(|line| line.strip_prefix("      - name: "))
        .collect::<Vec<_>>();
    let gate_index = step_names
        .iter()
        .position(|name| *name == gate_name)
        .unwrap_or_else(|| panic!("budget gate not found in code-review step list"));
    for step_name in &step_names[gate_index + 1..] {
        let step = step_body(&content, step_name);
        assert!(
            step.contains("steps.ocr-budget.outputs.should_run == 'true'"),
            "Post-budget step '{step_name}' must not run after the automatic OCR budget is exhausted"
        );
    }
}

#[test]
fn ocr_budget_uses_configured_post_open_limit_with_strict_defaulting() {
    let content = read_workflow();
    let gate = step_body(&content, "Check automatic OCR review budget");
    assert!(
        gate.contains("OCR_MAX_REVIEWS_POST_OPEN: ${{ vars.OCR_MAX_REVIEWS_POST_OPEN }}")
            && gate.contains("OCR_MAX_REVIEWS_POST_OPEN_DEFAULT: '2'"),
        "Budget gate must use OCR_MAX_REVIEWS_POST_OPEN with an explicit default of two"
    );
    assert!(
        gate.contains("/^\\d+$/")
            && gate.contains("Number.isSafeInteger")
            && gate.contains("core.setFailed"),
        "Only nonnegative decimal integer limits may pass budget validation"
    );
    assert!(
        gate.contains("currentCount >= limit"),
        "The automatic review must be skipped when its completed count reaches the configured limit, including limit zero"
    );
}

#[test]
fn ocr_budget_counts_marker_backed_reviews_once_and_migrates_persisted_state() {
    let content = read_workflow();
    let gate = step_body(&content, "Check automatic OCR review budget");
    for required in [
        "github.rest.pulls.listReviews",
        "github.rest.pulls.listReviewComments",
        "<!-- jefe-ocr-inline -->",
        "pull_request_review_id",
        "BOT_LOGINS",
        "isWorkflowBot(comment.user)",
        "isWorkflowBot(review.user)",
        "completedReviewCommits",
        "review.commit_id",
        "persistedMatch",
        "budgetComments.length > 1",
        "Number.isSafeInteger(persistedCount)",
    ] {
        assert!(
            gate.contains(required),
            "Budget gate must derive compatible completed-review state using {required:?}"
        );
    }
    assert!(
        gate.contains("github.paginate"),
        "Historical review and comment queries must paginate"
    );
    let persisted_branch = gate
        .find("if (persistedMatch)")
        .unwrap_or_else(|| panic!("persisted-state decision branch not found"));
    let legacy_query = gate
        .find("github.rest.pulls.listReviewComments")
        .unwrap_or_else(|| panic!("legacy migration query not found"));
    assert!(
        persisted_branch < legacy_query && gate.contains("currentCount = persistedCount"),
        "Persisted automatic state must be authoritative without legacy API dependencies"
    );
}

#[test]
fn ocr_budget_manual_triggers_bypass_and_never_increment_automatic_state() {
    let content = read_workflow();
    let gate = step_body(&content, "Check automatic OCR review budget");
    assert!(
        gate.contains("if (eventName !== 'pull_request_target')")
            && gate.contains("if (!budgetComment)")
            && gate.contains("core.setOutput('skipped', 'false')")
            && gate.contains("core.setOutput('should_run', 'true')"),
        "Manual triggers must initialize any migration baseline and run regardless of the automatic limit"
    );

    let reserve = step_body(&content, "Reserve automatic OCR review");
    assert!(
        reserve.contains("github.event_name == 'pull_request_target'")
            && reserve.contains("steps.ocr-budget.outputs.should_run == 'true'"),
        "Only an admitted automatic pull_request_target run may reserve an automatic slot"
    );
}

#[test]
fn ocr_budget_skip_notice_is_sticky_and_points_to_manual_review_commands() {
    let content = read_workflow();
    let gate = step_body(&content, "Check automatic OCR review budget");
    assert!(
        gate.contains("<!-- jefe-ocr-budget -->")
            && gate.contains("OCR skipped: post-open review budget")
            && gate.contains("/ocr")
            && gate.contains("/open-code-review"),
        "An exhausted automatic budget must publish one actionable marked notice"
    );
    assert!(
        gate.contains("github.rest.issues.updateComment")
            && gate.contains("github.rest.issues.createComment"),
        "The budget notice must update its existing marked comment or create it once"
    );
    assert!(
        gate.contains("core.setOutput('skipped', 'true')")
            && gate.contains("core.setOutput('should_run', 'false')")
            && content.contains("review_skipped: ${{ steps.ocr-budget.outputs.skipped }}"),
        "An exhausted automatic budget must explicitly mark the successful no-op and prevent later work"
    );
}

#[test]
fn ocr_budget_reserves_one_persistent_count_before_an_automatic_invocation() {
    let content = read_workflow();
    let reserve_position = content
        .find("- name: Reserve automatic OCR review")
        .unwrap_or_else(|| panic!("automatic OCR reservation step not found"));
    let review_position = content
        .find("- name: Run OpenCodeReview")
        .unwrap_or_else(|| panic!("OCR invocation step not found"));
    assert!(
        reserve_position < review_position,
        "Automatic count persistence must happen before OCR invocation"
    );

    let reserve = step_body(&content, "Reserve automatic OCR review");
    let review = step_body(&content, "Run OpenCodeReview");
    assert!(
        reserve.contains("ocr-exit-code.txt")
            && reserve.contains("core.setOutput('reserved', 'false')"),
        "Setup and preflight failures must not consume an automatic slot"
    );
    assert!(
        reserve.contains("currentCount + 1")
            && reserve.contains("<!-- jefe-ocr-budget -->")
            && reserve.contains("<!-- jefe-ocr-auto-count:")
            && reserve.contains("BOT_LOGINS")
            && reserve.contains("isWorkflowBot(comment.user)")
            && reserve.contains("github.rest.issues.updateComment")
            && !reserve.contains("github.rest.issues.createComment"),
        "An admitted automatic run must reserve exactly one slot in the initialized authenticated state"
    );
    assert!(
        review.contains("steps.ocr-reservation.outputs.reserved == 'true'")
            && review.contains("github.event_name != 'pull_request_target'"),
        "Automatic OCR must require a successful reservation while manual OCR bypasses it"
    );
}
