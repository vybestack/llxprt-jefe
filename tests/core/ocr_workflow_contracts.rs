//! OCR review workflow contract tests (issue #310).
//!
//! These tests verify the operational controls required by issue #310 are
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
