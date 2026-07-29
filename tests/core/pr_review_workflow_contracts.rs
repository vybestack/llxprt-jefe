//! LLxprt PR Review workflow contract tests (issue #474).
//!
//! Regression guard for the stale-`base.sha` checkout defect. The
//! `LLxprt PR Review` workflow (`.github/workflows/pr-review.yml`) runs
//! `node scripts/ci-quota-check.mjs` and `node scripts/pr-review-walkthrough.mjs`
//! against the working tree produced by its checkout step. Those scripts (and
//! their helpers) were introduced by commit `8d0c45b` (PR #466). When the
//! workflow checked out `github.event.pull_request.base.sha`, a PR whose
//! recorded base SHA was a strict ancestor of `8d0c45b` got a tree lacking
//! those scripts, causing `Cannot find module .../scripts/ci-quota-check.mjs`
//! (exit 1) deterministically across reruns.
//!
//! All `git diff` operations in the workflow use explicit SHAs
//! (`${MERGE_BASE}` and `${PR_HEAD_SHA}`), never the working tree, so the
//! checked-out ref's only role is to supply the workflow-supporting scripts.
//! The contract below asserts the checkout resolves scripts from the
//! base-branch tip (which always contains them) while `base.sha` is preserved
//! as the input to `git merge-base` for accurate diff scoping.
//!
//! These tests read the workflow as text (mirroring `ocr_workflow_contracts`)
//! so CI fails mechanically if the regression returns.

use std::path::{Path, PathBuf};

const WORKFLOW_PATH: &str = ".github/workflows/pr-review.yml";

/// Every script the workflow executes or the executed scripts import. The
/// checkout tree must contain all of these; this is the exact failure
/// surface from issue #474.
const WORKFLOW_SUPPORTING_SCRIPTS: &[&str] = &[
    "scripts/ci-quota-check.mjs",
    "scripts/pr-review-walkthrough.mjs",
    // Modules imported by pr-review-walkthrough.mjs (relative imports). If
    // the checkout lacks these, the walkthrough step fails with a module
    // resolution error after the quota step succeeds.
    "scripts/pr-review-prompts.mjs",
    "scripts/pr-review-llm-helpers.mjs",
    "scripts/pr-review-artifacts.mjs",
];

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}

fn read_workflow() -> String {
    let path = repo_path(WORKFLOW_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

/// Extract the body of a single named workflow step, starting from the
/// `- name: <step_name>` line until the next top-level `- name:` or
/// step-level key at a lower indentation. Mirrors `ocr_workflow_contracts`
/// so assertions bind to the named step and cannot be satisfied by comments
/// or unrelated steps.
fn step_body(content: &str, step_name: &str) -> String {
    // pr-review.yml uses single-quoted step names (`- name: 'Step'`), so match
    // both the quoted and unquoted forms of the `- name:` header.
    let needles = [
        format!("- name: {step_name}"),
        format!("- name: '{step_name}'"),
    ];
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|l| {
            let trimmed = l.trim();
            needles.iter().any(|needle| trimmed == *needle)
        })
        .unwrap_or_else(|| panic!("step '{step_name}' not found in workflow"));

    // Derive the step indentation from the first non-blank line after the
    // `- name:` header. Skipping blank lines avoids a zero-indent if a blank
    // line ever appears between the header and its first property, which
    // would otherwise cause the parser to consume all subsequent steps.
    let step_indent = lines[start + 1..]
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .find(|&indent| indent > 0)
        .unwrap_or(8);

    let mut body = String::new();
    body.push_str(lines[start]);
    body.push('\n');

    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
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
// Criterion A1 + A2: checkout resolves scripts from the base-branch tip
// ---------------------------------------------------------------------------

#[test]
fn checkout_does_not_use_stale_base_sha_as_ref() {
    // The checkout step must NOT use github.event.pull_request.base.sha as
    // its ref, because that SHA can be a strict ancestor of the commit that
    // introduced the workflow-supporting scripts, yielding a tree that lacks
    // them. The base-branch tip (base.ref) always contains the scripts.
    let content = read_workflow();
    let step = step_body(&content, "Checkout base revision");
    assert!(
        !step.contains("github.event.pull_request.base.sha"),
        "checkout step must not use the possibly-stale github.event.pull_request.base.sha as its ref \
         (issue #474: it can be older than the script-introducing commit and produce a tree lacking \
         scripts/ci-quota-check.mjs)"
    );
}

#[test]
fn checkout_resolves_base_branch_tip() {
    // The checkout ref must target the base branch so the working tree is
    // guaranteed to contain the workflow-supporting scripts regardless of how
    // stale the recorded base.sha is. base.ref is the base branch name and is
    // the natural source of the branch tip.
    let content = read_workflow();
    let step = step_body(&content, "Checkout base revision");
    assert!(
        step.contains("github.event.pull_request.base.ref"),
        "checkout step must resolve the base-branch tip via github.event.pull_request.base.ref so \
         the tree always contains the workflow-supporting scripts (issue #474)"
    );
}

// ---------------------------------------------------------------------------
// Criterion A2: every workflow-supporting script exists in the repository
// ---------------------------------------------------------------------------

#[test]
fn workflow_supporting_scripts_exist_in_repository() {
    // The scripts the workflow executes and imports must exist at the repo
    // tip. This both documents the failure surface and guards against a
    // future rename that would reintroduce a module-resolution failure.
    for script in WORKFLOW_SUPPORTING_SCRIPTS {
        let path = repo_path(script);
        assert!(
            path.is_file(),
            "workflow-supporting script {script} must exist in the repository",
        );
    }
}

// ---------------------------------------------------------------------------
// Criterion A3: base.sha is preserved as the git merge-base input
// ---------------------------------------------------------------------------

#[test]
fn base_sha_still_feeds_merge_base_for_diff_scoping() {
    // Diff scoping must remain anchored to the event's recorded base.sha via
    // git merge-base, independent of the checked-out ref. This preserves the
    // property that diffs contain only changes the PR actually introduced.
    let content = read_workflow();
    let step = step_body(&content, "Fetch pull request head");
    assert!(
        step.contains("github.event.pull_request.base.sha"),
        "the event's base.sha must still be read into base_sha for git merge-base diff scoping \
         (issue #474: the fix changes only the checkout ref, not the merge-base input)"
    );
    assert!(
        step.contains("git merge-base"),
        "base.sha must still feed git merge-base so diff scoping is unchanged"
    );
}
