//! Tests for the GitHub cleanup module (Finding #3, #4).
//!
//! Split from `github_executor_tests.rs` to keep file sizes under the hard
//! limit of 1000 lines.

use super::super::github_executor::{CommandRunner, TierBContext, TierBError};
use super::*;
use crate::tutorial_capture::allowlist::FixtureAllowlist;
use crate::tutorial_capture::manifest::{
    GitHubResource, GitHubResourceKind, RunId, RunManifest, RuntimeProfile,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

/// A fake command runner for testing that returns canned responses.
struct FakeCommandRunner {
    responses: RefCell<HashMap<String, String>>,
}

impl FakeCommandRunner {
    fn new() -> Self {
        let mut map = HashMap::new();
        // Default responses for gh issue/pr create — return URLs with numbers.
        map.insert(
            "issue create".to_string(),
            "https://github.com/fixture/test/issues/42".to_string(),
        );
        map.insert(
            "pr create".to_string(),
            "https://github.com/fixture/test/pull/7".to_string(),
        );
        Self {
            responses: RefCell::new(map),
        }
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(
        &mut self,
        program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        let key = format!("{program} {}", argv.first().unwrap_or(&String::new()));
        let responses = self.responses.borrow();
        if let Some(resp) = responses.get(&key) {
            return Ok(resp.clone());
        }
        // Check for sub-key matches (e.g. "issue create" in "gh issue create --repo ...").
        for (k, v) in responses.iter() {
            if key.contains(k.as_str()) {
                return Ok(v.clone());
            }
        }
        Ok(String::new())
    }
}

/// Import functions from github_executor for the tests.
use super::super::github_executor::{execute_tier_b, plan_tier_b, validate_clone_destination};

/// Create a test manifest with creation allowlist provenance set, as
/// required by the fail-closed cleanup logic.
fn make_test_manifest() -> RunManifest {
    let id = RunId::new("cleanup-test").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);
    manifest
}

// ─── Basic cleanup execution ────────────────────────────────────────

#[test]
fn execute_github_cleanup_runs_all_commands() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let outcomes =
        execute_github_cleanup(&manifest, &mut runner).value_or_panic("cleanup should succeed");
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].status, GithubCleanupStatus::Cleaned));
}

// ─── Finding #4: cleanup validation (skips block completion) ────────

/// Cleanup must SKIP resources with empty identifiers (fail-closed).
#[test]
fn cleanup_skips_resource_with_empty_identifier() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: String::new(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "skipped resources should return error",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Skipped { reason } if reason.contains("empty"))
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Cleanup must SKIP resources that don't belong to the fixture GitHub repo.
#[test]
fn cleanup_skips_resource_from_wrong_repo() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "other/repo".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "skipped should return error",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Skipped { reason } if reason.contains("does not match"))
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Cleanup must SKIP resources in production repos (fail-closed).
#[test]
fn cleanup_skips_production_repo_resource() {
    let mut manifest = make_test_manifest();
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "vybestack/jefe".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "skipped should return error",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Skipped { reason } if reason.contains("production"))
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Cleanup with an allowlist must SKIP resources not in the allowlist.
#[test]
fn cleanup_skips_resource_not_in_allowlist() {
    let mut manifest = make_test_manifest();
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let allowlist = FixtureAllowlist::new(["fixture/other"]);
    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup_with_allowlist(&manifest, &mut runner, Some(&allowlist)),
        "skipped should return error",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Skipped { reason } if reason.contains("allowlist"))
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Cleanup must return an error when a cleanup command fails, and per-resource
/// outcomes must show the failure.
struct FailingCleanupRunner;
impl CommandRunner for FailingCleanupRunner {
    fn run(
        &mut self,
        _program: &str,
        _argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        Err("network error".to_string())
    }
}

#[test]
fn cleanup_returns_error_on_command_failure_with_outcomes() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });

    let mut runner = FailingCleanupRunner;
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "should fail on command failure",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(matches!(
                &outcomes[0].status,
                GithubCleanupStatus::Failed { stderr } if stderr.contains("network error")
            ));
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

// ─── Finding #4: Clone destination containment validation ──────────

#[test]
fn validate_clone_destination_accepts_exact_fixture_clone() {
    let run_root = PathBuf::from("/tmp/jefe-run-001");
    let clone_dest = run_root.join("fixture-clone");
    validate_clone_destination(&clone_dest, &run_root)
        .value_or_panic("exact fixture-clone must be accepted");
}

#[test]
fn validate_clone_destination_rejects_arbitrary_external_path() {
    let run_root = PathBuf::from("/tmp/jefe-run-001");
    let clone_dest = PathBuf::from("/tmp/some-other-path");
    let err = error_or_panic(
        validate_clone_destination(&clone_dest, &run_root),
        "should reject external path",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationNotContained { .. }),
        "should reject: {err:?}"
    );
}

#[test]
fn validate_clone_destination_rejects_wrong_subdirectory() {
    let run_root = PathBuf::from("/tmp/jefe-run-001");
    let clone_dest = run_root.join("wrong-dir");
    let err = error_or_panic(
        validate_clone_destination(&clone_dest, &run_root),
        "should reject wrong subdir",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationNotContained { .. }),
        "should reject: {err:?}"
    );
}

#[test]
fn validate_clone_destination_rejects_path_traversal() {
    let run_root = PathBuf::from("/tmp/jefe-run-001");
    let clone_dest = run_root.join("..").join("fixture-clone");
    let err = error_or_panic(
        validate_clone_destination(&clone_dest, &run_root),
        "should reject path traversal",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationNotContained { .. }),
        "should reject: {err:?}"
    );
}

#[test]
fn validate_clone_destination_rejects_nul_byte() {
    let run_root = PathBuf::from("/tmp/jefe-run-001");
    let clone_dest = PathBuf::from("/tmp/jefe-run-001/fixture-clone\0evil");
    let err = error_or_panic(
        validate_clone_destination(&clone_dest, &run_root),
        "should reject NUL byte",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationNotContained { .. }),
        "should reject: {err:?}"
    );
}

#[test]
fn execute_tier_b_rejects_non_contained_clone_dest() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let run_root = PathBuf::from("/tmp/jefe-run-002");
    let dest = PathBuf::from("/tmp/wrong-clone-dest");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let mut manifest = make_test_manifest();
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root: &run_root,
        save_fn: &mut save_fn,
    };

    let err = error_or_panic(
        execute_tier_b(&plan, &mut manifest, &mut ctx),
        "should reject non-contained clone dest",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationNotContained { .. }),
        "should reject: {err:?}"
    );
}

// ─── Finding #3: Cleanup idempotent handling and creation-allowlist ──

/// A runner that simulates "already closed" responses for idempotent cleanup.
struct AlreadyClosedRunner;
impl CommandRunner for AlreadyClosedRunner {
    fn run(
        &mut self,
        _program: &str,
        _argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        Err("Error: issue is already closed".to_string())
    }
}

#[test]
fn cleanup_treats_already_closed_as_idempotent_success() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = AlreadyClosedRunner;
    let outcomes = execute_github_cleanup(&manifest, &mut runner)
        .value_or_panic("already-closed should be idempotent success");
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0].status, GithubCleanupStatus::Cleaned),
        "already-closed should be Cleaned"
    );
}

#[test]
fn cleanup_treats_already_deleted_branch_as_idempotent_success() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Branch,
        repository: "fixture/test".to_string(),
        identifier: "tutorial-capture/run-001".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = AlreadyDeletedRunner;
    let outcomes = execute_github_cleanup(&manifest, &mut runner)
        .value_or_panic("already-deleted should be idempotent success");
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0].status, GithubCleanupStatus::Cleaned),
        "already-deleted should be Cleaned"
    );
}

struct AlreadyDeletedRunner;
impl CommandRunner for AlreadyDeletedRunner {
    fn run(
        &mut self,
        _program: &str,
        _argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        Err("Error: reference does not exist".to_string())
    }
}

#[test]
fn cleanup_revalidates_against_creation_allowlist() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.set_creation_allowlist(vec!["other/repo".to_string()]);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "should fail because resource repo not in creation allowlist",
    );
    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            assert_eq!(outcomes.len(), 1);
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Skipped { reason } if reason.contains("creation-time allowlist")),
                "should skip because not in creation allowlist: got {:?}",
                outcomes[0].status
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

// ── Task #3: Allowlist provenance persistence and cleanup ────────────

/// Verify that the `FixtureAllowlist::normalized_repos` method returns
/// the normalized (lowercase, trimmed) repos for provenance persistence.
#[test]
fn allowlist_normalized_repos_are_lowercase_and_sorted() {
    let allowlist = FixtureAllowlist::new(["Fixture/Beta", "  fixture/alpha  "]);
    let repos = allowlist.normalized_repos();
    assert_eq!(repos, vec!["fixture/alpha", "fixture/beta"]);
}

/// Task #3: cleanup uses immutable manifest provenance, not env.
/// When the manifest has a creation_allowlist, resources not in it are
/// skipped even if no runtime allowlist is passed (None).
#[test]
fn cleanup_skips_resource_not_in_manifest_provenance_without_env_allowlist() {
    let mut manifest = RunManifest::new(
        RunId::new("prov-test").value_or_panic("valid id"),
        "0.0.28",
        "test",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("fixture/test");
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    // Add a resource NOT in the creation allowlist.
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "other/repo".to_string(),
        identifier: "99".to_string(),
        url: None,
        title: String::new(),
    });

    let mut runner = FakeCommandRunner::new();
    // Pass None for allowlist — cleanup should use manifest provenance only.
    let result = execute_github_cleanup_with_allowlist(&manifest, &mut runner, None);
    assert!(
        result.is_err(),
        "should fail because other/repo is not in creation allowlist"
    );
    match result {
        Err(TierBError::CleanupPartialFailure { outcomes }) => {
            let skipped = outcomes
                .iter()
                .filter(|o| matches!(&o.status, GithubCleanupStatus::Skipped { .. }))
                .count();
            assert_eq!(skipped, 1, "other/repo should be skipped");
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Task #3: CLI/file-only authorization then cleanup.
/// When the manifest has creation_allowlist from CLI/file sources (no env),
/// cleanup with no runtime allowlist (None) still cleans resources that ARE
/// in the manifest provenance.
#[test]
fn cleanup_cleans_resource_in_manifest_provenance_with_cli_only_auth() {
    let mut manifest = RunManifest::new(
        RunId::new("cli-auth-test").value_or_panic("valid id"),
        "0.0.28",
        "test",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("fixture/cli-test");
    // Simulate CLI/file-only authorization: no env, just explicit repos.
    manifest.set_creation_allowlist(vec!["fixture/cli-test".to_string()]);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/cli-test".to_string(),
        identifier: "5".to_string(),
        url: None,
        title: String::new(),
    });

    let mut runner = FakeCommandRunner::new();
    // Pass None — cleanup should use manifest provenance, not env.
    let result = execute_github_cleanup_with_allowlist(&manifest, &mut runner, None);
    assert!(
        result.is_ok(),
        "should succeed: resource is in manifest provenance"
    );
    let outcomes = result.unwrap_or_else(|e| panic!("should succeed: {e:?}"));
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0].status, GithubCleanupStatus::Cleaned),
        "resource in manifest provenance should be cleaned"
    );
}

/// Fail-closed: if GitHub resources exist but creation_allowlist is empty,
/// cleanup must refuse to proceed.
#[test]
fn cleanup_fails_closed_when_creation_allowlist_empty() {
    let id = RunId::new("fail-closed-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    // Do NOT set creation_allowlist — it's empty.
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });
    let mut runner = FakeCommandRunner::new();
    let result = execute_github_cleanup(&manifest, &mut runner);
    assert!(
        result.is_err(),
        "must fail closed when creation_allowlist is empty"
    );
    match result {
        Err(TierBError::CleanupPartialFailure { outcomes }) => {
            assert!(
                outcomes.iter().all(|o| matches!(
                    &o.status,
                    GithubCleanupStatus::Skipped { reason } if reason.contains("provenance")
                )),
                "all outcomes must be skipped with provenance reason"
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}

/// Custom runner that returns "reference does not exist" as an error,
/// simulating a branch auto-deleted by a PR merge.
struct MergedBranchRunner;
impl CommandRunner for MergedBranchRunner {
    fn run(
        &mut self,
        _program: &str,
        _argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        Err("reference does not exist".to_string())
    }
}

/// Idempotent: merged PR or auto-deleted branch should be treated as
/// already cleaned.
#[test]
fn cleanup_recognizes_merged_pr_as_idempotent() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Branch,
        repository: "fixture/test".to_string(),
        identifier: "feature-branch".to_string(),
        url: None,
        title: String::new(),
    });

    let mut runner = MergedBranchRunner;
    let result = execute_github_cleanup(&manifest, &mut runner);
    assert!(
        result.is_ok(),
        "merged/auto-deleted branch must be treated as idempotent success: {result:?}"
    );
    let outcomes = result.value_or_panic("should succeed");
    assert!(
        matches!(outcomes[0].status, GithubCleanupStatus::Cleaned),
        "auto-deleted branch must be cleaned (idempotent)"
    );
}

// ─── Issue #241: CleanupPartialFailure mixed outcomes ───────────────

/// Issue #241 regression: when cleanup produces mixed outcomes (one
/// resource cleans successfully, another is skipped), the returned
/// `CleanupPartialFailure` must contain ALL outcomes so the CLI can
/// process every one through `process_cleanup_outcome` and persist
/// the correct discrepancies.
#[test]
fn cleanup_partial_failure_contains_all_outcomes_when_mixed() {
    let mut manifest = make_test_manifest();
    manifest.set_fixture_github_repo("fixture/test");
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);

    // First resource: valid, will be cleaned.
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: None,
        title: String::new(),
    });

    // Second resource: empty identifier, will be skipped.
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: String::new(),
        url: None,
        title: String::new(),
    });

    // Third resource: wrong repo, will be skipped.
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "other/repo".to_string(),
        identifier: "99".to_string(),
        url: None,
        title: String::new(),
    });

    let mut runner = FakeCommandRunner::new();
    let err = error_or_panic(
        execute_github_cleanup(&manifest, &mut runner),
        "mixed outcomes should return CleanupPartialFailure",
    );

    match err {
        TierBError::CleanupPartialFailure { outcomes } => {
            // ALL outcomes must be present — not just the first error.
            assert_eq!(
                outcomes.len(),
                3,
                "CleanupPartialFailure must contain all 3 outcomes"
            );

            // First outcome: cleaned (valid resource, fake runner succeeds).
            assert!(
                matches!(&outcomes[0].status, GithubCleanupStatus::Cleaned),
                "first outcome must be Cleaned, got {:?}",
                outcomes[0].status
            );

            // Second outcome: skipped (empty identifier).
            assert!(
                matches!(&outcomes[1].status, GithubCleanupStatus::Skipped { reason } if reason.contains("empty")),
                "second outcome must be Skipped for empty identifier"
            );

            // Third outcome: skipped (wrong repo).
            assert!(
                matches!(&outcomes[2].status, GithubCleanupStatus::Skipped { reason } if reason.contains("does not match")),
                "third outcome must be Skipped for wrong repo"
            );
        }
        other => panic!("expected CleanupPartialFailure, got {other:?}"),
    }
}
