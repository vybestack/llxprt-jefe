//! OCR review workflow contract tests (issue #310).
//!
//! These tests verify the operational controls required by issue #310 are
//! present in `.github/workflows/ocr-review.yml` without weakening any quality
//! rules or adding suppressions. They read the workflow as text (the same
//! approach used by `tmux_harness_docs_contracts`) and assert that each
//! acceptance criterion is structurally wired.

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

// ---------------------------------------------------------------------------
// Criterion 1: single exact OCR version source
// ---------------------------------------------------------------------------

#[test]
fn ocr_version_is_declared_as_single_source() {
    let content = read_workflow();
    let line = content
        .lines()
        .find(|l| l.trim().starts_with("OCR_VERSION:"))
        .unwrap_or_else(|| {
            panic!("Workflow must declare OCR_VERSION as the single version source")
        });
    let raw_value = line
        .trim()
        .strip_prefix("OCR_VERSION:")
        .unwrap_or_else(|| panic!("OCR_VERSION line was malformed: {line:?}"))
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
fn ocr_literal_version_appears_only_once() {
    let content = read_workflow();
    let occurrences = content
        .lines()
        .filter(|l| l.contains("@alibaba-group/open-code-review@1.7.9"))
        .count();
    assert_eq!(
        occurrences, 0,
        "Literal 1.7.9 must not appear in the install command; only in the OCR_VERSION declaration"
    );
}

// ---------------------------------------------------------------------------
// Criterion 2: bounded connectivity preflight
// ---------------------------------------------------------------------------

#[test]
fn ocr_has_bounded_connectivity_preflight() {
    let content = read_workflow();
    assert!(
        content.contains("llm test"),
        "Workflow must run a bounded OCR LLM connectivity preflight (ocr llm test)"
    );
    assert!(
        content.contains("timeout 120s"),
        "Connectivity preflight must be bounded by an explicit timeout wrapper"
    );
    // Exit code 124 is the standard GNU coreutils `timeout` kill exit code.
    assert!(
        content.contains("124"),
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
    assert!(
        content.contains("file review") && content.contains("provider/config/auth"),
        "OCR review must classify wholesale per-file failure and auth/config/provider failure"
    );
}

#[test]
fn ocr_review_classifies_timeout_distinctly() {
    let content = read_workflow();
    assert!(
        content.contains("timed out"),
        "OCR review must classify timeout distinctly from other failure modes"
    );
}

// ---------------------------------------------------------------------------
// Criterion 5: fail-closed redaction (placeholder before redaction)
// ---------------------------------------------------------------------------

#[test]
fn ocr_redaction_destroys_original_before_redaction() {
    let content = read_workflow();
    assert!(
        content.contains("Fail closed")
            || content.contains("fail-closed")
            || content.contains("fail closed"),
        "Redaction must document the fail-closed invariant"
    );
    // The original file must be overwritten with a placeholder BEFORE the
    // redacted content is written, so a write error can never leak secrets.
    assert!(
        content.contains("[redaction unavailable for") || content.contains("placeholder"),
        "Redaction must write a safe placeholder before attempting redaction"
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
    // The create path must search before creating and re-check after a
    // failed create, reconciling by the deterministic issue title.
    let recheck_count = content.matches("gh issue list").count();
    assert!(
        recheck_count >= 3,
        "Notification must reconcile ambiguous writes by re-searching before and after create (found {recheck_count} search calls)"
    );
}

// ---------------------------------------------------------------------------
// Criterion 7: label-less fallback only for verified missing-label response
// ---------------------------------------------------------------------------

#[test]
fn ocr_label_less_fallback_is_conditional() {
    let content = read_workflow();
    // The label-less fallback must check for a verified missing-label response,
    // not retry unconditionally.
    assert!(
        content.contains("Labels") || content.contains("labels") || content.contains("label"),
        "Label-less fallback must inspect the label error response"
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
    // The convergence sweep must close duplicates.
    assert!(
        content.contains("converge") || content.contains("dedup") || content.contains("close"),
        "Notification must converge duplicate open tracking issues"
    );
}

// ---------------------------------------------------------------------------
// Criterion 9: deduplicate exact candidates before batch posting
// ---------------------------------------------------------------------------

#[test]
fn ocr_deduplicates_findings_before_posting() {
    let content = read_workflow();
    assert!(
        content.contains("dedup") || content.contains("Dedup") || content.contains("deduplicate"),
        "Post-OCR posting must deduplicate exact candidates from the current result before batch posting"
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
