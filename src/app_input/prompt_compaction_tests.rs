//! Tests for prompt-size compaction (issue #409).
//!
//! When an issue or PR body is large enough that the full inlined prompt would
//! approach the tmux pane-command length limit (~16 KB), the prompt body must
//! be compacted to a short preview + a `gh issue/pr view --comments` fetch
//! instruction instead of being truncated or causing a spawn failure.
//!
//! The agent runs in a checked-out git repo with `gh` available and
//! authenticated, so it can fetch the full live content itself.

use super::fresh_prompt::{
    FreshPromptKind, ISSUE_DELIVERY_WORKFLOW, MAX_PROMPT_CONTENT_BYTES,
    PROMPT_COMPACTION_THRESHOLD_BYTES, compact_prompt_content, fresh_prompt_instruction,
};

// ── Threshold consistency ────────────────────────────────────────────────

/// The compaction threshold must be strictly less than the max-content budget
/// so a prompt that is exactly at the threshold is NOT also truncated (which
/// would add a truncation marker on top of an already-compacted prompt).
const _: () = assert!(
    PROMPT_COMPACTION_THRESHOLD_BYTES < MAX_PROMPT_CONTENT_BYTES,
    "compaction threshold must be strictly below max content bytes to prevent double-processing"
);

// ── Large prompt produces compact reference ──────────────────────────────

/// A large prompt (content exceeds the threshold) must be compacted by
/// `fresh_prompt_instruction`: the instruction must NOT contain the full body,
/// and MUST contain a fetch reference (the compacted prompt is pre-built by
/// the caller using `compact_prompt_content`).
#[test]
fn large_issue_prompt_produces_gh_fetch_reference() {
    let large_body = "A".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 1000);
    let compacted =
        compact_prompt_content(&large_body, "gh issue view 42 --repo owner/repo --comments");

    assert!(
        !compacted.contains(&large_body),
        "compacted content must not contain the full large body"
    );
    assert!(
        compacted.contains("gh issue view"),
        "compacted content must include the gh fetch instruction:\n{compacted}"
    );
    // A preview of the body must still be present (first chunk).
    let preview_marker = "A".repeat(20);
    assert!(
        compacted.contains(&preview_marker),
        "compacted content must include a preview of the body content"
    );
    assert!(
        compacted.contains("compact reference"),
        "compacted content must signal the body was summarized:\n{compacted}"
    );
}

// ── Small prompt is inlined unchanged ───────────────────────────────────

/// A small prompt (under the threshold) must pass through `compact_prompt_content`
/// unchanged.
#[test]
fn small_prompt_is_inlined_unchanged() {
    let small_body = "This is a normal-sized issue body.";
    let compacted =
        compact_prompt_content(small_body, "gh issue view 42 --repo owner/repo --comments");

    assert_eq!(
        compacted, small_body,
        "small prompt must pass through unchanged"
    );
}

/// A large PR prompt must be compacted with a `gh pr view` reference.
#[test]
fn large_pr_prompt_produces_gh_fetch_reference() {
    let large_body = "B".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 500);
    let compacted =
        compact_prompt_content(&large_body, "gh pr view 42 --repo owner/repo --comments");

    assert!(
        !compacted.contains(&large_body),
        "compacted PR content must not contain the full large body"
    );
    assert!(
        compacted.contains("gh pr view"),
        "compacted PR content must include the gh fetch instruction:\n{compacted}"
    );
}

// ── Threshold-boundary prompt ────────────────────────────────────────────

/// A prompt exactly at the threshold must be inlined (boundary inclusive).
#[test]
fn prompt_at_exact_threshold_is_inlined() {
    let body = "C".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES);
    let compacted = compact_prompt_content(&body, "gh issue view 1 --repo o/r --comments");

    assert_eq!(
        compacted, body,
        "prompt at exactly the threshold must be inlined (not compacted)"
    );
}

/// A prompt one byte over the threshold must be compacted.
#[test]
fn prompt_one_byte_over_threshold_is_compacted() {
    let body = "D".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 1);
    let compacted = compact_prompt_content(&body, "gh issue view 1 --repo o/r --comments");

    assert!(
        !compacted.contains(&body),
        "prompt one byte over threshold must be compacted"
    );
    assert!(
        compacted.contains("gh issue view"),
        "one-byte-over prompt must have gh fetch reference"
    );
}

// ── Compaction preserves the issue delivery workflow ─────────────────────

/// The ISSUE_DELIVERY_WORKFLOW appendix must still be appended for compacted
/// issue prompts — compaction only replaces the body, not the workflow rules.
#[test]
fn compacted_issue_prompt_still_includes_delivery_workflow() {
    // Build a compacted prompt body, then pass it through fresh_prompt_instruction
    // to verify the workflow is still appended.
    let large_body = "E".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 1000);
    let compacted_body =
        compact_prompt_content(&large_body, "gh issue view 99 --repo owner/repo --comments");
    let instruction = fresh_prompt_instruction(FreshPromptKind::Issue, &compacted_body);

    // Assert against the constant itself rather than hardcoded fragments so
    // the test stays valid if the workflow wording evolves.
    assert!(
        instruction.contains(ISSUE_DELIVERY_WORKFLOW),
        "compacted issue prompt must still include the full ISSUE_DELIVERY_WORKFLOW"
    );
}

// ── Compacted prompt stays under tmux pane limit ────────────────────────

/// The compacted instruction (with workflow) must stay well under the tmux
/// pane-command limit (~16,340 bytes). This is the core invariant that
/// prevents the spawn failure (#409).
#[test]
fn compacted_prompt_with_workflow_stays_under_tmux_pane_limit() {
    // Even a 100 KB body must compact down to something safe.
    let huge_body = "X".repeat(100_000);
    let compacted_body =
        compact_prompt_content(&huge_body, "gh issue view 999 --repo owner/repo --comments");
    let instruction = fresh_prompt_instruction(FreshPromptKind::Issue, &compacted_body);

    assert!(
        instruction.len() < jefe::runtime::pane_command_budget().bytes,
        "compacted issue instruction must stay under the measured pane-command \
         budget ({} bytes), got {} bytes",
        jefe::runtime::pane_command_budget().bytes,
        instruction.len()
    );
}

// ── format_issue_prompt compacts large body ─────────────────────────────

/// The issue prompt formatter must compact the body when it is large,
/// replacing it with a preview + `gh issue view` reference, while keeping
/// metadata (title, repo, state, labels) and instructions inline.
#[test]
fn format_issue_prompt_compacts_large_body() {
    use super::issues_dispatch::format_issue_prompt;
    use jefe::github::SendPayload;

    let large_body = "Y".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 2000);
    let payload = SendPayload {
        repository: "owner/repo".to_owned(),
        issue_number: 409,
        issue_title: "Large issue causes tmux spawn failure".to_owned(),
        issue_body: large_body.clone(),
        issue_state: "OPEN".to_owned(),
        issue_labels: vec!["bug".to_owned()],
        issue_assignees: vec![],
        focused_comment: None,
        focused_comment_author: None,
        issue_base_prompt: "Fix the root cause.".to_owned(),
    };

    let prompt = format_issue_prompt(&payload);

    // The full body must NOT appear.
    assert!(
        !prompt.contains(&large_body),
        "format_issue_prompt must not inline the full large body"
    );
    // Must contain a gh fetch instruction with the repo and number.
    assert!(
        prompt.contains("gh issue view 409 --repo owner/repo --comments"),
        "compacted issue prompt must include gh issue view with repo/number:\n{prompt}"
    );
    // Metadata must still be inline.
    assert!(
        prompt.contains("owner/repo") && prompt.contains("#409"),
        "compacted prompt must keep repo and number metadata inline"
    );
    // Title must be present (it's small, always inline).
    assert!(
        prompt.contains("Large issue causes tmux spawn failure"),
        "compacted prompt must keep the issue title inline"
    );
    // Base prompt (instructions) must be inline.
    assert!(
        prompt.contains("Fix the root cause."),
        "compacted prompt must keep the base prompt (instructions) inline"
    );
    // A preview of the body must be present.
    assert!(
        prompt.contains(&"Y".repeat(20)),
        "compacted prompt must include a body preview"
    );
}

/// Small issue body must be inlined verbatim by format_issue_prompt.
#[test]
fn format_issue_prompt_inlines_small_body() {
    use super::issues_dispatch::format_issue_prompt;
    use jefe::github::SendPayload;

    let payload = SendPayload {
        repository: "owner/repo".to_owned(),
        issue_number: 1,
        issue_title: "Small issue".to_owned(),
        issue_body: "Just a small body.".to_owned(),
        issue_state: "OPEN".to_owned(),
        issue_labels: vec![],
        issue_assignees: vec![],
        focused_comment: None,
        focused_comment_author: None,
        issue_base_prompt: String::new(),
    };

    let prompt = format_issue_prompt(&payload);
    assert!(
        prompt.contains("Just a small body."),
        "small body must be inlined verbatim"
    );
    assert!(
        !prompt.contains("gh issue view"),
        "small body must not trigger compaction"
    );
}

// ── format_pr_prompt compacts large body ─────────────────────────────────

/// The PR prompt formatter must compact the body when it is large, using
/// `gh pr view` for the fetch reference.
#[test]
fn format_pr_prompt_compacts_large_body() {
    use super::prs_dispatch::format_pr_prompt;
    use jefe::github::PrSendPayload;

    let large_body = "Z".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 2000);
    let payload = PrSendPayload {
        repository: "owner/repo".to_owned(),
        pr_number: 42,
        pr_title: "Large PR".to_owned(),
        pr_body: large_body.clone(),
        pr_state: "OPEN".to_owned(),
        head_ref: "feature".to_owned(),
        base_ref: "main".to_owned(),
        external_url: String::new(),
        review_summary: vec![],
        check_summary: vec![],
        focused_comment: None,
        focused_comment_author: None,
        pr_base_prompt: "Review the diff.".to_owned(),
    };

    let prompt = format_pr_prompt(&payload);

    assert!(
        !prompt.contains(&large_body),
        "format_pr_prompt must not inline the full large body"
    );
    assert!(
        prompt.contains("gh pr view 42 --repo owner/repo --comments"),
        "compacted PR prompt must include gh pr view with repo/number:\n{prompt}"
    );
    assert!(
        prompt.contains("Review the diff."),
        "compacted PR prompt must keep base prompt inline"
    );
}

/// A large focused comment on an issue must also be compacted.
#[test]
fn format_issue_prompt_compacts_large_focused_comment() {
    use super::issues_dispatch::format_issue_prompt;
    use jefe::github::SendPayload;

    let large_comment = "Q".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 1000);
    let payload = SendPayload {
        repository: "owner/repo".to_owned(),
        issue_number: 5,
        issue_title: "Issue".to_owned(),
        issue_body: "small body".to_owned(),
        issue_state: "OPEN".to_owned(),
        issue_labels: vec![],
        issue_assignees: vec![],
        focused_comment: Some(large_comment.clone()),
        focused_comment_author: Some("reviewer".to_owned()),
        issue_base_prompt: String::new(),
    };

    let prompt = format_issue_prompt(&payload);

    assert!(
        !prompt.contains(&large_comment),
        "large focused comment must not be inlined verbatim"
    );
    assert!(
        prompt.contains("gh issue view 5 --repo owner/repo --comments"),
        "compacted comment must reference gh issue view:\n{prompt}"
    );
}

/// A large focused comment on a PR must also be compacted.
#[test]
fn format_pr_prompt_compacts_large_focused_comment() {
    use super::prs_dispatch::format_pr_prompt;
    use jefe::github::PrSendPayload;

    let large_comment = "W".repeat(PROMPT_COMPACTION_THRESHOLD_BYTES + 1000);
    let payload = PrSendPayload {
        repository: "owner/repo".to_owned(),
        pr_number: 7,
        pr_title: "PR".to_owned(),
        pr_body: "small body".to_owned(),
        pr_state: "OPEN".to_owned(),
        head_ref: "feature".to_owned(),
        base_ref: "main".to_owned(),
        external_url: String::new(),
        review_summary: vec![],
        check_summary: vec![],
        focused_comment: Some(large_comment.clone()),
        focused_comment_author: Some("reviewer".to_owned()),
        pr_base_prompt: String::new(),
    };

    let prompt = format_pr_prompt(&payload);

    assert!(
        !prompt.contains(&large_comment),
        "large focused PR comment must not be inlined verbatim"
    );
    assert!(
        prompt.contains("gh pr view 7 --repo owner/repo --comments"),
        "compacted PR comment must reference gh pr view:\n{prompt}"
    );
}

// -- Budget derivation (issue #540 V4) ------------------------------------

/// The prompt ceiling is derived from the measured pane-command budget, so a
/// re-measurement moves it rather than silently invalidating it. The reserve
/// covers the framing that shares the same command line.
#[test]
fn the_prompt_ceiling_is_derived_from_the_measured_budget() {
    let budget = jefe::runtime::pane_command_budget().bytes;

    assert!(
        MAX_PROMPT_CONTENT_BYTES < budget,
        "prompt ceiling {MAX_PROMPT_CONTENT_BYTES} must stay under the budget {budget}",
    );
    assert!(
        budget - MAX_PROMPT_CONTENT_BYTES >= 4_000,
        "the framing around the prompt needs headroom; budget {budget} leaves only {}",
        budget - MAX_PROMPT_CONTENT_BYTES,
    );
}

/// Compaction happens well before truncation, and both sit inside the budget.
/// Sizing compaction against tmux was how a macOS measurement came to govern a
/// Windows launch.
#[test]
fn compaction_engages_well_inside_the_budget() {
    let budget = jefe::runtime::pane_command_budget().bytes;

    // That compaction precedes truncation is already proven at compile time by
    // the `const _` assertion above; what needs asserting here is the relation
    // to the measured budget.
    assert!(
        PROMPT_COMPACTION_THRESHOLD_BYTES * 2 < budget,
        "compaction must leave room for the framing it does not control: \
         threshold {PROMPT_COMPACTION_THRESHOLD_BYTES}, budget {budget}",
    );
}
