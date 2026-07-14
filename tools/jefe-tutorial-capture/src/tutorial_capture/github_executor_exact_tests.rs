//! Exact resource validation tests (issue #241 Finding #2).
//!
//! Tests for strict canonical URL parsing, exact title persistence, exact
//! branch naming convention, URL/kind/repo matching, deduplication, and
//! backward-compatible title field deserialization.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use super::*;
use crate::manifest::{RunId, RunManifest, RuntimeProfile};
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

/// Re-use the fake runner from the main test module.
struct FakeCommandRunner {
    commands: Vec<(String, Vec<String>, Option<PathBuf>)>,
    outputs: HashMap<String, String>,
}

impl FakeCommandRunner {
    fn new() -> Self {
        let mut outputs = HashMap::new();
        outputs.insert(
            "Create fixture issue".to_string(),
            "https://github.com/fixture/test/issues/42\n".to_string(),
        );
        outputs.insert(
            "Clone fixture repo".to_string(),
            "Cloning into fixture/test...\n".to_string(),
        );
        outputs.insert("Create fixture branch".to_string(), String::new());
        outputs.insert("Stage changed file".to_string(), String::new());
        outputs.insert("Commit fixture change".to_string(), String::new());
        outputs.insert("Push fixture branch".to_string(), String::new());
        outputs.insert(
            "Create fixture PR".to_string(),
            "https://github.com/fixture/test/pull/7\n".to_string(),
        );
        Self {
            commands: Vec::new(),
            outputs,
        }
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(
        &mut self,
        program: &str,
        argv: &[String],
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        self.commands.push((
            program.to_string(),
            argv.to_vec(),
            cwd.map(Path::to_path_buf),
        ));
        let key = if argv.contains(&"issue".to_string()) && argv.contains(&"create".to_string()) {
            "Create fixture issue"
        } else if argv.contains(&"clone".to_string()) {
            if let Some(dest) = argv.last() {
                let _ = std::fs::create_dir_all(dest);
            }
            "Clone fixture repo"
        } else if argv.contains(&"checkout".to_string()) {
            "Create fixture branch"
        } else if argv.contains(&"add".to_string()) {
            "Stage changed file"
        } else if argv.contains(&"commit".to_string()) {
            "Commit fixture change"
        } else if argv.contains(&"push".to_string()) {
            "Push fixture branch"
        } else if argv.contains(&"pr".to_string()) && argv.contains(&"create".to_string()) {
            "Create fixture PR"
        } else {
            ""
        };
        self.outputs
            .get(key)
            .cloned()
            .ok_or_else(|| format!("no fake output for '{key}'"))
    }
}

/// `parse_github_resource_url` must parse a canonical GitHub URL.
#[test]
fn parse_canonical_issue_url() {
    let parsed = parse_github_resource_url("https://github.com/fixture/test/issues/42")
        .value_or_panic("canonical issue URL must parse");
    assert_eq!(parsed.repo, "fixture/test");
    assert_eq!(parsed.kind, GitHubResourceKind::Issue);
    assert_eq!(parsed.number, "42");
}

#[test]
fn parse_canonical_pr_url() {
    let parsed = parse_github_resource_url("https://github.com/fixture/test/pull/7")
        .value_or_panic("canonical PR URL must parse");
    assert_eq!(parsed.repo, "fixture/test");
    assert_eq!(parsed.kind, GitHubResourceKind::PullRequest);
    assert_eq!(parsed.number, "7");
}

#[test]
fn parse_url_rejects_non_numeric_id() {
    assert!(parse_github_resource_url("https://github.com/fixture/test/issues/abc").is_none());
}

#[test]
fn parse_url_rejects_non_github_or_non_https_host() {
    assert!(parse_github_resource_url("git@github.com:fixture/test.git").is_none());
    assert!(parse_github_resource_url("https://github.example/fixture/test/issues/42").is_none());
    assert!(parse_github_resource_url("https://gitlab.com/fixture/test/issues/42").is_none());
}

#[test]
fn parse_url_rejects_empty_output() {
    assert!(parse_github_resource_url("").is_none());
    assert!(parse_github_resource_url("   \n").is_none());
}

#[test]
fn parse_url_rejects_extra_path_segments() {
    assert!(parse_github_resource_url("https://github.com/fixture/test/issues/42/files").is_none());
}

/// Recorded issue resource has the exact title from the mutation plan.
#[test]
fn recorded_issue_resource_has_exact_title() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-001").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    let issue = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Issue)
        .value_or_panic("must have issue resource");
    assert_eq!(issue.title, plan.mutation_plan.issue_title);
    assert_eq!(issue.repository, "fixture/test");
    assert_eq!(issue.identifier, "42");
    assert_eq!(
        issue.url.as_deref(),
        Some("https://github.com/fixture/test/issues/42")
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Recorded PR resource has the exact title from the mutation plan.
#[test]
fn recorded_pr_resource_has_exact_title() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-002", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-002").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    let pr = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::PullRequest)
        .value_or_panic("must have PR resource");
    assert_eq!(pr.title, plan.mutation_plan.pr_title);
    assert_eq!(pr.repository, "fixture/test");
    assert_eq!(pr.identifier, "7");
    assert_eq!(
        pr.url.as_deref(),
        Some("https://github.com/fixture/test/pull/7")
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Branch resource has the exact `tutorial-capture/<run-id>` identifier.
#[test]
fn recorded_branch_resource_has_exact_convention() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-003", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-003").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    let branch = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Branch)
        .value_or_panic("must have branch resource");
    assert_eq!(
        branch.identifier, "tutorial-capture/exact-003",
        "branch must follow exact tutorial-capture/<run-id> convention"
    );
    assert_eq!(branch.repository, "fixture/test");

    let _ = std::fs::remove_dir_all(&base);
}

/// Issue/PR URL must match the plan repository and resource kind.
#[test]
fn recorded_resource_urls_match_repo_and_kind() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-004", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-004").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    for resource in &manifest.github_resources {
        if let Some(url) = &resource.url {
            let parsed = parse_github_resource_url(url)
                .value_or_panic("recorded URL must be parseable as canonical GitHub URL");
            assert_eq!(parsed.repo, resource.repository);
            assert_eq!(parsed.kind, resource.kind);
        }
    }

    let _ = std::fs::remove_dir_all(&base);
}

/// Exactly one each of issue, branch, PR — no duplicates.
#[test]
fn execute_produces_exactly_one_each_no_duplicates() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-005", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-005").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    let issue_count = manifest
        .github_resources
        .iter()
        .filter(|r| r.kind == GitHubResourceKind::Issue)
        .count();
    let branch_count = manifest
        .github_resources
        .iter()
        .filter(|r| r.kind == GitHubResourceKind::Branch)
        .count();
    let pr_count = manifest
        .github_resources
        .iter()
        .filter(|r| r.kind == GitHubResourceKind::PullRequest)
        .count();
    assert_eq!(issue_count, 1, "exactly one issue");
    assert_eq!(branch_count, 1, "exactly one branch");
    assert_eq!(pr_count, 1, "exactly one PR");

    let _ = std::fs::remove_dir_all(&base);
}

/// A runner that returns a bare number instead of a URL for issue create.
struct NonUrlRunner;

impl CommandRunner for NonUrlRunner {
    fn run(
        &mut self,
        _program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        if argv.contains(&"issue".to_string()) && argv.contains(&"create".to_string()) {
            Ok("42\n".to_string())
        } else if argv.contains(&"clone".to_string()) {
            if let Some(d) = argv.last() {
                let _ = std::fs::create_dir_all(d);
            }
            Ok(String::new())
        } else {
            Ok(String::new())
        }
    }
}

/// A non-URL gh output (e.g. just a number) is rejected, and execution fails.
#[test]
fn execute_fails_when_issue_output_is_not_url() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "exact-006", false, &dest).value_or_panic("plan");
    let id = RunId::new("exact-006").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);

    let mut runner = NonUrlRunner;
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let err = execute_tier_b(&plan, &mut manifest, &mut ctx)
        .err()
        .value_or_panic("should fail on non-URL output");
    assert!(
        err.to_string().contains("could not parse") || err.to_string().contains("URL"),
        "error must mention URL parsing: {err}"
    );
    let recovery = std::fs::read_to_string(run_root.join("github-mutation-recovery.json"))
        .value_or_panic("successful mutation must leave durable recovery identity");
    assert!(recovery.contains("fixture/test"));
    assert!(recovery.contains("fixture issue for documentation capture"));
    assert!(recovery.contains("42"));

    let _ = std::fs::remove_dir_all(&base);
}

/// GitHubResource title field roundtrips through JSON serialization.
#[test]
fn github_resource_title_round_trips_through_json() {
    let resource = GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: Some("https://github.com/fixture/test/issues/42".to_string()),
        title: "[tutorial-capture:run-001] fixture issue".to_string(),
    };
    let json = serde_json::to_string(&resource).value_or_panic("serialize");
    let reloaded: GitHubResource = serde_json::from_str(&json).value_or_panic("deserialize");
    assert_eq!(reloaded.title, resource.title);
}

/// Old JSON without title field deserializes with empty default (backcompat).
#[test]
fn github_resource_title_defaults_empty_for_backcompat() {
    let old_json = r#"{"kind":"issue","repository":"fixture/test","identifier":"42","url":"https://github.com/fixture/test/issues/42"}"#;
    let resource: GitHubResource =
        serde_json::from_str(old_json).value_or_panic("deserialize old JSON");
    assert_eq!(resource.title, "", "missing title should default to empty");
}
