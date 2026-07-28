//! Release workflow hardening contract tests (issue #471).
//!
//! These tests verify the durable CI/supply-chain hardening contracts deferred
//! from PR #444 (OCR run 4). They read `.github/workflows/release.yml` as text
//! (the same `read_repo_text` / `repo_path` pattern used by
//! `windows_support_contracts.rs` and `ocr_workflow_contracts.rs`) and assert
//! that each accepted behavior is structurally present. Assertions are scoped
//! to specific job blocks or the whole-workflow surface so unrelated steps or
//! comments cannot satisfy a contract check.
//!
//! Covered acceptance rows (see `project-plans/issue471-plan.md`):
//! - A1: third-party actions are pinned to full commit SHAs.
//! - A2: every release job declares a `timeout-minutes`.
//! - A3: `permissions: contents: write` is scoped to the publishing job only.
//! - A4: the Homebrew tap job explicitly installs `jq`.
//! - A5: the tap push does not assume a hardcoded default branch.
//! - A6: the Windows packaging/checksum steps from issue #264 are unchanged.

use std::path::{Path, PathBuf};

const WORKFLOW_PATH: &str = ".github/workflows/release.yml";

fn read_workflow() -> String {
    let path = repo_path(WORKFLOW_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}

/// Extract the body of a single top-level job, starting at `  <job-id>:` and
/// ending at the next job key at the same (two-space) indentation. This scopes
/// assertions so content from sibling jobs cannot satisfy a job-scoped check.
fn job_body(content: &str, job_id: &str) -> String {
    let header = format!("  {job_id}:");
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == header)
        .unwrap_or_else(|| panic!("job '{job_id}' not found in release workflow"));

    let mut body = String::new();
    body.push_str(lines[start]);
    body.push('\n');

    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body.push_str(line);
            body.push('\n');
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // A new top-level job is introduced by a two-space-indented key.
        if indent == 2 {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

// ---------------------------------------------------------------------------
// A6 (regression guard): the issue #264 Windows packaging and checksum steps
// remain unchanged. These must keep passing while the hardening edits land.
// ---------------------------------------------------------------------------

#[test]
fn windows_packaging_and_checksum_steps_are_preserved() {
    let workflow = read_workflow();
    assert!(
        workflow.contains("Package Windows portable zip"),
        "release.yml must retain the Windows portable-zip packaging step"
    );
    assert!(
        workflow.contains("Generate Windows checksums") && workflow.contains("Get-FileHash"),
        "release.yml must retain the Windows checksum generation step"
    );
    assert!(
        workflow.contains("x86_64-pc-windows-msvc"),
        "release.yml must retain the Windows MSVC matrix entry"
    );
}

// ---------------------------------------------------------------------------
// A1: every third-party action reference is pinned to a full 40-char SHA.
// ---------------------------------------------------------------------------

/// Known third-party actions used by the release workflow. A reference is
/// considered safe only when it pins to a full lowercase 40-character hex SHA.
const THIRD_PARTY_ACTIONS: &[&str] = &[
    "dtolnay/rust-toolchain",
    "Swatinem/rust-cache",
    "softprops/action-gh-release",
];

#[test]
fn third_party_actions_are_pinned_to_full_commit_shas() {
    let workflow = read_workflow();

    for action in THIRD_PARTY_ACTIONS {
        for line in workflow.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("- uses: ") else {
                continue;
            };
            let Some(_) = rest.strip_prefix(action) else {
                continue;
            };
            // `rest` starts with `<action>@<ref>...` possibly followed by a
            // comment. Isolate the ref token.
            let after_at = rest
                .strip_prefix(action)
                .unwrap_or("")
                .trim_start_matches('@');
            let ref_token = after_at.split_whitespace().next().unwrap_or(after_at);
            assert!(
                ref_token.len() == 40
                    && ref_token.chars().all(|c| c.is_ascii_hexdigit())
                    && ref_token == ref_token.to_ascii_lowercase(),
                "third-party action {action} must be pinned to a full 40-char commit SHA, found @{ref_token}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// A2: every release job declares a `timeout-minutes`.
// ---------------------------------------------------------------------------

#[test]
fn every_release_job_declares_a_timeout() {
    let workflow = read_workflow();
    for job_id in [
        "build-release-binaries",
        "publish-release-assets",
        "update-homebrew-tap",
    ] {
        let body = job_body(&workflow, job_id);
        assert!(
            body.lines()
                .any(|l| l.trim_start().starts_with("timeout-minutes:")),
            "release job '{job_id}' must declare a timeout-minutes"
        );
    }
}

// ---------------------------------------------------------------------------
// A3: `permissions: contents: write` is scoped to the publishing job only.
// ---------------------------------------------------------------------------

#[test]
fn workflow_default_permissions_are_read_only() {
    let workflow = read_workflow();
    // The top-level `permissions:` block appears at two-space indentation in
    // the workflow header, before any job. It must not grant contents: write.
    let header_end = workflow
        .find("\njobs:")
        .unwrap_or_else(|| panic!("release workflow must define a jobs: section"));
    let header = &workflow[..header_end];
    assert!(
        !header.contains("contents: write"),
        "the workflow-level permissions block must not grant contents: write; scope it to the publishing job"
    );
    assert!(
        header.contains("contents: read"),
        "the workflow-level permissions block must default to contents: read"
    );
}

#[test]
fn contents_write_is_scoped_to_the_publishing_job_only() {
    let workflow = read_workflow();

    let publish = job_body(&workflow, "publish-release-assets");
    assert!(
        publish.contains("contents: write"),
        "the publish-release-assets job must carry permissions: contents: write"
    );

    for job_id in ["build-release-binaries", "update-homebrew-tap"] {
        let body = job_body(&workflow, job_id);
        assert!(
            !body.contains("contents: write"),
            "release job '{job_id}' must not carry contents: write; only the publishing job needs it"
        );
    }
}

// ---------------------------------------------------------------------------
// A4: the Homebrew tap job explicitly installs jq before using it.
// ---------------------------------------------------------------------------

#[test]
fn homebrew_tap_job_explicitly_installs_jq() {
    let workflow = read_workflow();
    let tap = job_body(&workflow, "update-homebrew-tap");
    assert!(
        tap.contains("jq"),
        "the update-homebrew-tap job must reference jq"
    );
    // An explicit install step must precede the formula generation step that
    // consumes jq. Look for an install directive naming jq in the tap job.
    assert!(
        tap.lines().any(|l| {
            let t = l.trim_start();
            (t.contains("apt-get install") || t.contains("apt-get -y install")) && t.contains("jq")
        }) || tap.contains("Install jq"),
        "the update-homebrew-tap job must explicitly install jq rather than rely on the runner image"
    );
}

// ---------------------------------------------------------------------------
// A5: the tap push does not assume a hardcoded default branch.
// ---------------------------------------------------------------------------

#[test]
fn homebrew_tap_push_does_not_hardcode_main() {
    let workflow = read_workflow();
    let tap = job_body(&workflow, "update-homebrew-tap");
    assert!(
        !tap.contains("git push origin main"),
        "the update-homebrew-tap job must not push to a hardcoded 'main' default branch"
    );
    // The job must resolve the tap default branch dynamically via the GitHub
    // API default_branch field before pushing.
    assert!(
        tap.contains("default_branch"),
        "the update-homebrew-tap job must resolve the tap default branch dynamically (default_branch)"
    );
}
