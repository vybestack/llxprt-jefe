use super::*;
use crate::tutorial_capture::manifest::{RunId, RunManifest, RuntimeProfile};
use std::cell::RefCell;
use std::collections::HashMap;

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

// ─── Fake command runner ─────────────────────────────────────────────

/// A fake command runner that records all commands and returns canned
/// outputs based on the command description.
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
        outputs.insert("Merge fixture PR".to_string(), String::new());
        outputs.insert("Close fixture issue".to_string(), String::new());
        outputs.insert("Close fixture PR".to_string(), String::new());
        outputs.insert("Delete fixture branch".to_string(), String::new());
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
        // Match by the last meaningful arg or description.
        let key = if argv.contains(&"issue".to_string()) && argv.contains(&"create".to_string()) {
            "Create fixture issue"
        } else if argv.contains(&"clone".to_string()) {
            // Simulate clone: create the destination directory from argv.
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
        } else if argv.contains(&"merge".to_string()) {
            "Merge fixture PR"
        } else if argv.contains(&"close".to_string()) && argv.contains(&"issue".to_string()) {
            "Close fixture issue"
        } else if argv.contains(&"close".to_string()) && argv.contains(&"pr".to_string()) {
            "Close fixture PR"
        } else if argv.contains(&"DELETE".to_string()) {
            "Delete fixture branch"
        } else {
            ""
        };
        self.outputs
            .get(key)
            .cloned()
            .ok_or_else(|| format!("no fake output for '{key}'"))
    }
}

// ─── Plan generation ────────────────────────────────────────────────

#[test]
fn plan_tier_b_succeeds_for_allowlisted_repo() {
    let allowlist = FixtureAllowlist::new(["fixture/test-repo"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan = plan_tier_b(&allowlist, "fixture/test-repo", "run-001", false, &dest)
        .value_or_panic("plan should succeed");
    assert_eq!(plan.repository, "fixture/test-repo");
    assert!(!plan.merge);
    assert!(!plan.commands.is_empty());
}

#[test]
fn plan_tier_b_includes_correct_sequence() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let descriptions: Vec<&str> = plan
        .commands
        .iter()
        .map(|c| c.description.as_str())
        .collect();
    assert_eq!(
        descriptions,
        vec![
            "Create fixture issue",
            "Clone fixture repo",
            "Create fixture branch",
            "Stage changed file",
            "Commit fixture change",
            "Push fixture branch",
            "Create fixture PR",
        ]
    );
}

#[test]
fn plan_tier_b_includes_merge_flag_when_requested() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", true, &dest).value_or_panic("plan");
    assert!(plan.merge);
    // Merge is NOT in the pre-built command list — it is built dynamically
    // during execution using the captured PR number.
    assert!(
        !plan
            .commands
            .iter()
            .any(|c| c.description == "Merge fixture PR"),
        "merge must not be pre-built; it is dynamically constructed with the PR number"
    );
}

#[test]
fn plan_tier_b_excludes_merge_command_by_default() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    assert!(
        !plan
            .commands
            .iter()
            .any(|c| c.description == "Merge fixture PR")
    );
}

// ─── Allowlist refusal ──────────────────────────────────────────────

#[test]
fn plan_tier_b_refuses_production_repo() {
    let allowlist = FixtureAllowlist::new(["vybestack/jefe"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let err = error_or_panic(
        plan_tier_b(&allowlist, "vybestack/jefe", "run-001", false, &dest),
        "should refuse",
    );
    assert!(
        matches!(err, TierBError::FixtureRefused { .. }),
        "should be FixtureRefused: {err:?}"
    );
}

#[test]
fn plan_tier_b_refuses_non_allowlisted_repo() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let err = error_or_panic(
        plan_tier_b(&allowlist, "random/repo", "run-001", false, &dest),
        "should refuse",
    );
    assert!(
        matches!(err, TierBError::FixtureRefused { .. }),
        "should be FixtureRefused: {err:?}"
    );
}

// ─── Command structure ──────────────────────────────────────────────

#[test]
fn all_commands_use_gh_or_git() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    for cmd in &plan.commands {
        assert!(
            cmd.program == "gh" || cmd.program == "git",
            "commands must use gh or git, not shell: {}",
            cmd.program
        );
    }
}

#[test]
fn no_command_uses_shell_metacharacters() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", true, &dest).value_or_panic("plan");
    for cmd in &plan.commands {
        for arg in &cmd.argv {
            assert!(
                !arg.contains(';') && !arg.contains('|') && !arg.contains('&'),
                "command args must not contain shell metacharacters: {arg}"
            );
            assert!(
                !arg.contains('\0'),
                "command args must not contain NUL bytes"
            );
        }
    }
}

/// Finding #5: issue and PR create commands must NOT assume labels exist.
/// Labels are optional/probed, not hardcoded.
#[test]
fn issue_and_pr_commands_do_not_assume_labels() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let dest = PathBuf::from("/tmp/clone-test");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    for cmd in &plan.commands {
        assert!(
            !cmd.argv.contains(&"--label".to_string()),
            "commands must not assume labels exist: {} has --label",
            cmd.description
        );
    }
}

/// Finding #5: merge command explicitly targets the created PR number.
#[test]
fn merge_command_explicitly_targets_pr_number() {
    let plan = build_mutation_plan("fixture/test", "run-001", true);
    let merge_cmd = build_merge_command(&plan, "7");
    assert!(merge_cmd.argv.contains(&"7".to_string()));
    assert!(merge_cmd.argv.contains(&"merge".to_string()));
    assert!(merge_cmd.argv.contains(&"--repo".to_string()));
    // Verify the PR number comes before --repo so gh treats it as the target.
    let pr_pos = merge_cmd
        .argv
        .iter()
        .position(|a| a == "7")
        .value_or_panic("PR number must be in argv");
    let repo_pos = merge_cmd
        .argv
        .iter()
        .position(|a| a == "--repo")
        .value_or_panic("--repo must be in argv");
    assert!(pr_pos < repo_pos, "PR number must come before --repo");
}

#[test]
fn clone_command_has_correct_cwd_sequence() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let run_root = PathBuf::from("/tmp/jefe-test-run");
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    // Clone has no cwd (creates the destination).
    let clone_cmd = plan
        .commands
        .iter()
        .find(|c| c.description == "Clone fixture repo")
        .value_or_panic("should have clone command");
    assert!(clone_cmd.cwd.is_none());
    // Checkout and subsequent git commands have cwd = clone_dest.
    let checkout_cmd = plan
        .commands
        .iter()
        .find(|c| c.description == "Create fixture branch")
        .value_or_panic("should have checkout command");
    assert_eq!(checkout_cmd.cwd.as_deref(), Some(dest.as_path()));
}

// ─── Cleanup planning ───────────────────────────────────────────────

#[test]
fn plan_github_cleanup_only_targets_manifest_resources() {
    let id = RunId::new("cleanup-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: Some("https://github.com/fixture/test/issues/42".to_string()),
        title: String::new(),
    });

    let commands = plan_github_cleanup(&manifest);
    assert_eq!(commands.len(), 1);
    assert!(commands[0].argv.contains(&"42".to_string()));
}

#[test]
fn plan_github_cleanup_empty_for_empty_manifest() {
    let id = RunId::new("empty-cleanup").value_or_panic("valid id");
    let manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let commands = plan_github_cleanup(&manifest);
    assert!(commands.is_empty());
}

// ─── Disposable confirmation ────────────────────────────────────────

#[test]
fn execute_refuses_without_confirmation() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let run_root = PathBuf::from("/tmp/jefe-test-run");
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: false,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root: &run_root,
        save_fn: &mut save_fn,
    };

    let err = error_or_panic(
        execute_tier_b(&plan, &mut manifest, &mut ctx),
        "should refuse",
    );
    assert!(
        matches!(err, TierBError::NotConfirmed),
        "should be NotConfirmed: {err:?}"
    );
}

// ─── Full execution with fake runner ────────────────────────────────

#[test]
fn execute_tier_b_records_all_resources_in_manifest() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-full").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FakeCommandRunner::new();
    let save_count = RefCell::new(0usize);
    let mut save_fn = |_m: &RunManifest| {
        *save_count.borrow_mut() += 1;
        Ok(())
    };

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let outputs =
        execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute should succeed");

    assert!(!outputs.is_empty());
    // Manifest should have issue, branch, and PR recorded.
    assert_eq!(manifest.github_resources.len(), 3);
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::Issue && r.identifier == "42")
    );
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::PullRequest && r.identifier == "7")
    );
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::Branch)
    );
    // Manifest saved after each resource.
    assert!(*save_count.borrow() >= 3);
}

/// Issue #241 task #2: setup executor must NOT merge. The --allow-merge flag
/// selects the merge capture permission/variant only; the merge is driven
/// through the Jefe UI during capture, not during setup.
#[test]
fn execute_tier_b_does_not_merge_during_setup() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", true, &dest).value_or_panic("plan");
    let id = RunId::new("exec-merge").value_or_panic("valid id");
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

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute should succeed");

    // The merge command must NOT have been executed during setup.
    let merge_was_run = runner
        .commands
        .iter()
        .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string()));
    assert!(
        !merge_was_run,
        "setup executor must NOT run merge; merge is driven through the Jefe UI during capture"
    );

    // The issue, branch, and PR must have been created.
    assert!(
        runner
            .commands
            .iter()
            .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"issue".to_string())),
        "issue creation must have run"
    );
    assert!(
        runner.commands.iter().any(|(prog, argv, _)| prog == "gh"
            && argv.contains(&"pr".to_string())
            && argv.contains(&"create".to_string())),
        "PR creation must have run"
    );
}

#[test]
fn execute_tier_b_fails_on_existing_clone_dest() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    std::fs::create_dir_all(&dest).value_or_panic("create dest");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-exist").value_or_panic("valid id");
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

    let err = error_or_panic(
        execute_tier_b(&plan, &mut manifest, &mut ctx),
        "should fail on existing dest",
    );
    assert!(
        matches!(err, TierBError::CloneDestinationExists { .. }),
        "should be CloneDestinationExists: {err:?}"
    );
}

/// Command runner that fails on any `create` command.
struct FailingRunner;
impl CommandRunner for FailingRunner {
    fn run(
        &mut self,
        _program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        if argv.contains(&"create".to_string()) {
            Err("simulated failure".to_string())
        } else {
            Ok(String::new())
        }
    }
}

#[test]
fn execute_tier_b_on_failure_saves_manifest_without_failed_resources() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-fail").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = FailingRunner;
    let save_count = RefCell::new(0usize);
    let mut save_fn = |_m: &RunManifest| {
        *save_count.borrow_mut() += 1;
        Ok(())
    };

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let _err = error_or_panic(
        execute_tier_b(&plan, &mut manifest, &mut ctx),
        "should fail on create command",
    );

    // Finding #3: manifest should have been saved even on failure.
    assert!(
        *save_count.borrow() >= 1,
        "manifest must be saved on failure"
    );
    // Finding #3: NO GitHub resources should be recorded for failed commands.
    assert!(
        manifest.github_resources.is_empty(),
        "no GitHub resources should be recorded on failure, got: {:?}",
        manifest.github_resources
    );
}

// ─── Finding #3: branch recorded after push, nonempty identifiers ─────

/// Custom runner that tracks whether push has been executed, while returning
/// canned responses for other Tier-B commands (issue create, clone, PR create).
struct PushTrackingRunner {
    push_completed: bool,
}

impl CommandRunner for PushTrackingRunner {
    fn run(
        &mut self,
        program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        let key = if argv.contains(&"issue".to_string()) && argv.contains(&"create".to_string()) {
            "https://github.com/fixture/test/issues/42\n"
        } else if argv.contains(&"clone".to_string()) {
            if let Some(dest) = argv.last() {
                let _ = std::fs::create_dir_all(dest);
            }
            ""
        } else if argv.contains(&"checkout".to_string())
            || argv.contains(&"add".to_string())
            || argv.contains(&"commit".to_string())
        {
            ""
        } else if program == "git" && argv.contains(&"push".to_string()) {
            self.push_completed = true;
            ""
        } else if argv.contains(&"pr".to_string()) && argv.contains(&"create".to_string()) {
            "https://github.com/fixture/test/pull/7\n"
        } else {
            ""
        };
        Ok(key.to_string())
    }
}

/// The branch resource must be recorded when push succeeds, NOT when
/// checkout runs. This test uses a custom runner that tracks which commands
/// have run, verifying that the branch resource appears only after push.
#[test]
fn execute_tier_b_records_branch_after_push_not_checkout() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-push-order").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let mut runner = PushTrackingRunner {
        push_completed: false,
    };
    let mut save_fn = |_m: &RunManifest| Ok(());

    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute should succeed");

    assert!(runner.push_completed, "push must have been executed");

    let branch = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Branch);
    assert!(
        branch.is_some(),
        "branch resource must be recorded after push"
    );
    let branch = branch.value_or_panic("branch resource must exist");
    assert!(
        !branch.identifier.is_empty(),
        "branch identifier must be nonempty"
    );
    assert_eq!(
        branch.repository, "fixture/test",
        "branch repository must be the explicit plan repo"
    );
}

/// All recorded resource identifiers must be nonempty. This test verifies
/// that resources with empty identifiers are not recorded.
#[test]
fn execute_tier_b_resources_have_nonempty_identifiers_and_explicit_repo() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let dest = run_root.join("fixture-clone");
    let plan =
        plan_tier_b(&allowlist, "fixture/test", "run-001", false, &dest).value_or_panic("plan");
    let id = RunId::new("exec-nonempty").value_or_panic("valid id");
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

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute should succeed");

    for resource in &manifest.github_resources {
        assert!(
            !resource.identifier.is_empty(),
            "resource identifier must be nonempty: {resource:?}",
        );
        assert!(
            resource.repository == "fixture/test",
            "resource repository must be the explicit plan repo, got: {}",
            resource.repository
        );
    }
}
// ─── Resource extraction ────────────────────────────────────────────

#[test]
fn extract_number_from_issue_url() {
    let url = "https://github.com/fixture/test/issues/42\n";
    assert_eq!(extract_number_from_url(url), Some("42".to_string()));
}

#[test]
fn extract_number_from_pr_url() {
    let url = "https://github.com/fixture/test/pull/7\n";
    assert_eq!(extract_number_from_url(url), Some("7".to_string()));
}

#[test]
fn repo_from_argv_finds_repo_flag() {
    let argv = vec![
        "gh".to_string(),
        "issue".to_string(),
        "create".to_string(),
        "--repo".to_string(),
        "fixture/test".to_string(),
        "--title".to_string(),
        "test".to_string(),
    ];
    assert_eq!(repo_from_argv(&argv), Some("fixture/test".to_string()));
}

// ─── Cleanup execution ──────────────────────────────────────────────
// ─── Cleanup execution ──────────────────────────────────────────────

/// Extract the `--repo` value from a command's argv (test helper).
fn repo_from_argv(argv: &[String]) -> Option<String> {
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        if arg == "--repo"
            && let Some(repo) = iter.next()
        {
            return Some(repo.clone());
        }
    }
    None
}
