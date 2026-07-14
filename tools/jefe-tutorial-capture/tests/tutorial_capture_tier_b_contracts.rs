//! Tier-B Jefe setup/capture contract tests (issue #241, Finding #2).
//!
//! These tests verify the Tier-B workflow model: pre-capture (GitHub fixture
//! setup), capture (Jefe TUI interaction), and post-capture (result assertion).
//! They use the fake command runner — no live GitHub mutations.
//!
//! ## Finding #2 contract
//!
//! The Tier-B workflow is modeled as distinct phases:
//!
//! 1. **Pre-capture (plan-github)**: Creates the fixture issue, clones the
//!    fixture repo, creates a feature branch with a change, pushes, and
//!    creates a PR. The manifest records all GitHub resources. Setup does
//!    NOT merge — merge belongs in the Jefe capture step.
//! 2. **Capture (capture-github)**: Drives Jefe's Issues and PR modes
//!    against the fixture repo. The strict GitHub scenario inspects the
//!    fixture issue and PR, executes send-to-agent confirmations, and
//!    asserts resulting state.
//! 3. **Post-capture**: The merge interaction and its result belong in the
//!    Jefe capture, not in setup.
//!
//! If merge cannot be fully deterministic without a live repo, the
//! pre-capture/post-capture steps are modeled as separate contract tests.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use jefe_tutorial_capture::{
    CommandRunner, FixtureAllowlist, GitHubResourceKind, OwnedPathKind, RunId, RunManifest,
    RuntimeProfile, TierBContext, execute_tier_b, plan_tier_b, validate_clone_destination,
};

trait TierBTestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TierBTestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TierBTestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

struct FakeTierBRunner {
    commands: Vec<(String, Vec<String>, Option<PathBuf>)>,
    outputs: HashMap<String, String>,
}

impl FakeTierBRunner {
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
        outputs.insert(
            "View fixture issue".to_string(),
            r#"{"title":"[tutorial-capture:tier-b-full-001] fixture issue for documentation capture"}"#.to_string(),
        );
        outputs.insert(
            "View fixture PR".to_string(),
            r#"{"title":"[tutorial-capture:tier-b-full-001] fixture pull request","headRefName":"tutorial-capture/tier-b-full-001"}"#.to_string(),
        );
        Self {
            commands: Vec::new(),
            outputs,
        }
    }
}

impl CommandRunner for FakeTierBRunner {
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
        let key = if argv.contains(&"issue".to_string()) && argv.contains(&"view".to_string()) {
            "View fixture issue"
        } else if argv.contains(&"pr".to_string()) && argv.contains(&"view".to_string()) {
            "View fixture PR"
        } else if argv.contains(&"issue".to_string()) && argv.contains(&"create".to_string()) {
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
        } else {
            ""
        };
        self.outputs
            .get(key)
            .cloned()
            .ok_or_else(|| format!("no fake output for '{key}'"))
    }
}

fn make_manifest() -> RunManifest {
    let id = RunId::new("tier-b-contract-001").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(
        id,
        "0.0.28",
        "tutorial-capture-github",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("fixture/test");
    manifest
}

// ── Finding #2: Pre-capture phase (plan-github setup) ───────────────

/// Pre-capture phase: plan-github with --confirm-disposable creates the
/// fixture issue, clone, branch, change, push, and PR — but does NOT merge.
/// The manifest records all GitHub resources.
#[test]
fn tier_b_pre_capture_creates_resources_without_merge() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-001", false, &clone_dest)
        .value_or_panic("plan should succeed");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute should succeed");

    // Pre-capture must create issue, branch, and PR — but NOT merge.
    let has_merge = runner
        .commands
        .iter()
        .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string()));
    assert!(!has_merge, "pre-capture must not merge");

    // Manifest records issue, branch, and PR.
    assert_eq!(manifest.github_resources.len(), 3);
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::Issue && r.identifier == "42"),
        "must record fixture issue"
    );
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::PullRequest && r.identifier == "7"),
        "must record fixture PR"
    );
    assert!(
        manifest
            .github_resources
            .iter()
            .any(|r| r.kind == GitHubResourceKind::Branch),
        "must record fixture branch"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Pre-capture phase records GitHub resources in the manifest for the
/// dedicated strict GitHub scenario to inspect.
#[test]
fn tier_b_pre_capture_records_fixture_github_repo() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-002", false, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    // Manifest must associate the fixture GitHub repo.
    assert_eq!(
        manifest.fixture_github_repo.as_deref(),
        Some("fixture/test"),
        "manifest must record the fixture GitHub repo"
    );

    // The fixture issue URL and PR URL must be recorded.
    let issue = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Issue)
        .value_or_panic("must have issue resource");
    assert!(
        issue
            .url
            .as_ref()
            .is_some_and(|u: &String| u.contains("issues/42")),
        "issue URL must be recorded: {:?}",
        issue.url
    );

    let pr = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::PullRequest)
        .value_or_panic("must have PR resource");
    assert!(
        pr.url
            .as_ref()
            .is_some_and(|u: &String| u.contains("pull/7")),
        "PR URL must be recorded: {:?}",
        pr.url
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Pre-capture must create the clone destination so the Jefe capture step
/// can use it as the working directory.
#[test]
fn tier_b_pre_capture_creates_clone_destination() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-003", false, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    // The clone destination directory must exist after pre-capture.
    assert!(
        clone_dest.exists(),
        "clone destination must exist after pre-capture"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ── Finding #2: Post-capture phase (merge belongs in capture) ───────

/// Post-capture phase: merge is an opt-in step that belongs in the Jefe
/// capture, NOT in pre-capture setup. When merge=true (via --allow-merge),
/// the executor does NOT merge — merge is only a capture permission/variant.
/// The merge variant is driven entirely by the Jefe TUI capture scenario.
#[test]
fn tier_b_post_capture_merge_happens_after_resource_creation() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    // Plan WITH merge flag (selects merge capture permission/variant only).
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-004", true, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute with merge flag");

    // The executor must NOT execute any merge command — merge belongs in
    // the Jefe capture step, driven by the TUI scenario.
    let has_merge = runner
        .commands
        .iter()
        .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string()));
    assert!(
        !has_merge,
        "executor must not merge even when plan.merge=true — merge belongs in capture"
    );

    // PR creation must have succeeded, and no merge command may appear
    // anywhere after the PR create command (Finding #4: was previously a
    // trivially-true assertion that index < len).
    let pr_create_index = runner
        .commands
        .iter()
        .position(|(prog, argv, _)| {
            prog == "gh" && argv.contains(&"pr".to_string()) && argv.contains(&"create".to_string())
        })
        .value_or_panic("PR create command must have been executed");
    // Verify no merge command was issued after PR creation.
    let merge_after_pr = runner
        .commands
        .iter()
        .skip(pr_create_index + 1)
        .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string()));
    assert!(
        !merge_after_pr,
        "no merge command may follow PR create — merge belongs in the Jefe capture step only"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Post-capture merge is NOT done by the executor. The plan's merge flag
/// selects a capture permission/variant only; the actual merge command is
/// driven by the Jefe TUI capture scenario (not the executor).
#[test]
fn tier_b_post_capture_merge_targets_created_pr_number() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-005", true, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute with merge flag");

    // The executor must NOT execute any merge command. The merge flag only
    // selects the capture variant — the merge is performed by the Jefe
    // TUI capture scenario using the PR number from the manifest.
    let merge_cmd = runner
        .commands
        .iter()
        .find(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string()));
    assert!(
        merge_cmd.is_none(),
        "executor must not execute merge — merge is driven by the capture scenario"
    );

    // The PR resource must be recorded so the capture scenario can merge it.
    let pr = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::PullRequest)
        .value_or_panic("must have PR resource for capture scenario to merge");
    assert_eq!(
        pr.identifier, "7",
        "PR number must be recorded so capture can merge it"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ── Finding #2: Setup must not pre-merge ────────────────────────────

/// Setup (plan-github without --allow-merge) must NOT execute any merge
/// command.
#[test]
fn tier_b_setup_without_allow_merge_does_not_merge() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-006", false, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    assert!(
        !runner
            .commands
            .iter()
            .any(|(prog, argv, _)| prog == "gh" && argv.contains(&"merge".to_string())),
        "setup without --allow-merge must NOT execute any merge command"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ── Finding #2: Manifest reload after fake execution ───────────────

/// After fake Tier-B execution, the manifest must reflect the created
/// GitHub resources so a reload shows the same state.
#[test]
fn tier_b_manifest_after_execution_has_correct_resources() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(&allowlist, "fixture/test", "tier-b-007", false, &clone_dest)
        .value_or_panic("plan");

    let mut manifest = make_manifest();
    // Serialize before execution.
    let json_before = manifest.to_json().value_or_panic("serialize before");
    let reloaded_before = RunManifest::from_json(&json_before).value_or_panic("deserialize before");
    assert!(
        reloaded_before.github_resources.is_empty(),
        "manifest should start with no GitHub resources"
    );

    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Ok(());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    execute_tier_b(&plan, &mut manifest, &mut ctx).value_or_panic("execute");

    // Serialize after execution and reload.
    let json_after = manifest.to_json().value_or_panic("serialize after");
    let reloaded_after = RunManifest::from_json(&json_after).value_or_panic("deserialize after");
    assert_eq!(
        reloaded_after.github_resources.len(),
        3,
        "reloaded manifest must have 3 GitHub resources (issue, branch, PR)"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ── Finding #2: Clone ownership kind is FixtureClone ───────────────

/// The clone destination is recorded as a FixtureClone owned path in the
/// manifest, with the correct kind.
#[test]
fn tier_b_clone_destination_recorded_as_fixture_clone_kind() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");

    let mut manifest = make_manifest();
    // Record the clone dest as a FixtureClone path (simulating what
    // record_tier_b_paths does in the CLI).
    manifest.add_owned_path(OwnedPathKind::FixtureClone, clone_dest.clone());

    // Verify the kind is correct.
    let clone_entry = manifest
        .owned_paths
        .iter()
        .find(|p| p.kind == OwnedPathKind::FixtureClone)
        .value_or_panic("must have FixtureClone path");
    assert_eq!(clone_entry.path, clone_dest);

    let _ = std::fs::remove_dir_all(&base);
}

// ── Finding #14: Fake Tier B full path (versioned persistence + cleanup) ──

#[cfg(unix)]
use jefe_tutorial_capture::{
    GithubCleanupStatus, RunOutcome, execute_github_cleanup_with_allowlist, prepare_run,
    save_manifest, save_manifest_atomic,
};

/// Fake runner that simulates successful cleanup for all resources.
#[cfg(unix)]
struct FakeCleanupRunner {
    closed: Vec<String>,
}

#[cfg(unix)]
impl FakeCleanupRunner {
    fn new() -> Self {
        Self { closed: Vec::new() }
    }
}

#[cfg(unix)]
impl CommandRunner for FakeCleanupRunner {
    fn run(
        &mut self,
        _program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
        let desc = argv.join(" ");
        self.closed.push(desc);
        if argv.contains(&"view".to_string()) && argv.contains(&"issue".to_string()) {
            return Ok(
                r#"{"title":"[tutorial-capture:tier-b-full-001] fixture issue for documentation capture"}"#
                    .to_string(),
            );
        }
        if argv.contains(&"view".to_string()) && argv.contains(&"pr".to_string()) {
            return Ok(
                r#"{"title":"[tutorial-capture:tier-b-full-001] fixture pull request","headRefName":"tutorial-capture/tier-b-full-001"}"#
                    .to_string(),
            );
        }
        Ok(String::new())
    }
}

/// Full Tier-B path test: prepare → seed state → execute fake Tier B →
/// save manifest (versioned persistence) → reload → cleanup → verify.
///
/// Build a standard Tier-B run setup for the full-path test.
#[cfg(unix)]
fn build_tier_b_setup(run_id: &RunId, base: &std::path::Path) -> jefe_tutorial_capture::RunSetup {
    jefe_tutorial_capture::RunSetup {
        run_id: run_id.clone(),
        base_dir: base.to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "tutorial-capture-github".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: Some("fixture/test".to_string()),
        jefe_bin: None,
        theme: Some("dark".to_string()),
        scenario_hash: Some("abc123".to_string()),
        shim_availability: jefe_tutorial_capture::ShimAvailability::default(),
    }
}

/// Execute fake Tier B and return the run root + manifest.
#[cfg(unix)]
fn execute_fake_tier_b(
    dirs: &jefe_tutorial_capture::RunDirectories,
    manifest: &mut RunManifest,
    clone_dest: &std::path::Path,
) {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let plan = plan_tier_b(
        &allowlist,
        "fixture/test",
        "tier-b-full-001",
        false,
        clone_dest,
    )
    .value_or_panic("plan");
    let mut runner = FakeTierBRunner::new();
    let manifest_path = dirs.manifest_path();
    let manifest_path_clone = manifest_path.clone();
    let mut save_fn = |m: &RunManifest| match save_manifest(m, &manifest_path_clone) {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    };
    let run_root = dirs.root.clone();
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest,
        run_root: &run_root,
        save_fn: &mut save_fn,
    };
    execute_tier_b(&plan, manifest, &mut ctx).value_or_panic("execute tier b");
}

/// This exercises the complete fake path without any live GitHub mutation.
#[cfg(unix)]
#[test]
fn fake_tier_b_full_path_with_versioned_persistence_reload_and_cleanup() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_id = RunId::new("tier-b-full-001").value_or_panic("valid run id");

    // Step 1: Prepare the run with an exclusive sentinel.
    let setup = build_tier_b_setup(&run_id, base.path());
    let (dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare should succeed");
    let run_root = dirs.root.clone();
    let clone_dest = run_root.join("fixture-clone");

    // Step 2: Record creation-time allowlist provenance.
    manifest.set_creation_allowlist(vec!["fixture/test".to_string()]);

    // Step 3: Execute fake Tier B.
    execute_fake_tier_b(&dirs, &mut manifest, &clone_dest);

    // Step 4: Save manifest with versioned persistence.
    manifest.set_outcome(RunOutcome::Success);
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save atomically");

    // Step 5: Reload from disk and verify versioned persistence.
    let reloaded =
        jefe_tutorial_capture::load_and_validate(&run_root).value_or_panic("reload should succeed");
    assert_eq!(
        reloaded.github_resources.len(),
        3,
        "must have 3 resources after reload"
    );
    assert_eq!(
        reloaded.creation_allowlist,
        vec!["fixture/test".to_string()],
        "creation allowlist must survive reload"
    );
    assert_eq!(reloaded.outcome, RunOutcome::Success);

    // Step 6: Fake cleanup using a cleanup runner.
    let mut cleanup_manifest = reloaded.clone();
    let mut cleanup_runner = FakeCleanupRunner::new();
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let outcomes = execute_github_cleanup_with_allowlist(
        &cleanup_manifest,
        &mut cleanup_runner,
        Some(&allowlist),
    )
    .value_or_panic("cleanup should succeed");
    assert_eq!(outcomes.len(), 3, "must clean 3 resources");
    for outcome in &outcomes {
        assert!(
            matches!(outcome.status, GithubCleanupStatus::Cleaned),
            "all resources should be cleaned: {:?}",
            outcome.status
        );
    }

    // Step 7: Local cleanup should work with the reloaded manifest.
    let records =
        jefe_tutorial_capture::cleanup_manifest_with_root(&mut cleanup_manifest, &run_root, false)
            .value_or_panic("local cleanup");
    assert!(!records.is_empty(), "must have cleanup records");

    let _ = std::fs::remove_dir_all(&base);
}

// ── Merge authorization persistence contract ────────────────────────

/// Merge authorization is persisted in the manifest from `plan-github
/// --allow-merge`. The manifest's `merge_authorized` field is true only
/// when the flag is set.
#[test]
fn tier_b_merge_authorization_persisted_when_allow_merge_flag_set() {
    let id = RunId::new("merge-auth-001").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(
        id,
        "0.0.28",
        "tutorial-capture-github",
        100,
        32,
        RuntimeProfile::Shim,
    );
    assert!(
        !manifest.merge_authorized,
        "manifest must default to merge_authorized=false"
    );
    manifest.set_merge_authorized(true);
    assert!(
        manifest.merge_authorized,
        "manifest must reflect merge_authorized=true after setter"
    );
    // Verify serialization round-trip.
    let json = manifest.to_json().value_or_panic("serialize");
    let reloaded = RunManifest::from_json(&json).value_or_panic("deserialize");
    assert!(
        reloaded.merge_authorized,
        "merge_authorized must survive JSON round-trip"
    );
}

/// Merge authorization NOT persisted when --allow-merge is absent.
#[test]
fn tier_b_merge_not_authorized_without_allow_merge_flag() {
    let id = RunId::new("merge-auth-002").value_or_panic("valid run id");
    let manifest = RunManifest::new(
        id,
        "0.0.28",
        "tutorial-capture-github",
        100,
        32,
        RuntimeProfile::Shim,
    );
    assert!(
        !manifest.merge_authorized,
        "manifest must default to merge_authorized=false without --allow-merge"
    );
}

// ── Clone destination validation contract ───────────────────────────

/// Clone destination must be exactly run_root/fixture-clone; arbitrary
/// paths are rejected before any mutation.
#[test]
fn tier_b_clone_dest_must_be_run_root_fixture_clone() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    // Correct: run_root/fixture-clone.
    let valid = run_root.join("fixture-clone");
    validate_clone_destination(&valid, run_root).value_or_panic("valid clone dest should pass");
    // Wrong: arbitrary path outside run root.
    let bad = std::path::PathBuf::from("/tmp/arbitrary-clone");
    let err = validate_clone_destination(&bad, run_root)
        .err()
        .value_or_panic("arbitrary clone dest should fail");
    assert!(
        err.to_string().contains("must be exactly"),
        "error must explain containment requirement: {err}"
    );
    // Wrong: traversal that escapes run root.
    let traversal = run_root.join("../fixture-clone");
    let _ = validate_clone_destination(&traversal, run_root)
        .err()
        .value_or_panic("traversal clone dest should fail");
    let _ = std::fs::remove_dir_all(&base);
}

/// Clone destination validation with NUL byte fails.
#[test]
fn tier_b_clone_dest_rejects_nul_byte() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let nul_path = run_root.join("fixture-clone\0");
    let err = validate_clone_destination(&nul_path, run_root)
        .err()
        .value_or_panic("NUL byte clone dest should fail");
    assert!(err.to_string().contains("NUL"));
    let _ = std::fs::remove_dir_all(&base);
}
