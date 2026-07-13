//! Tier-B fatal manifest save contracts (issue #241, Finding #3).
//!
//! These tests verify that all manifest saves in capture/validation are
//! fatal/nonzero. The `execute_tier_b` save callback must propagate errors
//! rather than silently ignoring them.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::collections::HashMap;
use std::path::Path;

use jefe::tutorial_capture::{
    CommandRunner, FixtureAllowlist, RunId, RunManifest, RuntimeProfile, TierBContext,
    execute_tier_b, plan_tier_b,
};

trait ResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

struct FakeTierBRunner {
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
        Self { outputs }
    }
}

impl CommandRunner for FakeTierBRunner {
    fn run(
        &mut self,
        _program: &str,
        argv: &[String],
        _cwd: Option<&Path>,
    ) -> Result<String, String> {
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

fn make_manifest() -> RunManifest {
    let id = RunId::new("fatal-save-001").value_or_panic("valid run id");
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

/// Finding #3: If the save_fn returns an error during Tier B execution,
/// execute_tier_b must propagate that error rather than silently ignoring it.
/// The manifest save is fatal.
#[test]
fn tier_b_execute_propagates_save_fn_error() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(
        &allowlist,
        "fixture/test",
        "tier-b-save-001",
        false,
        &clone_dest,
    )
    .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    let mut save_fn = |_m: &RunManifest| Err("injected persistence failure".to_string());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let err = execute_tier_b(&plan, &mut manifest, &mut ctx)
        .err()
        .value_or_panic("save_fn failure should propagate as error");
    let msg = err.to_string();
    assert!(
        msg.contains("save manifest") || msg.contains("persistence"),
        "error must mention manifest save failure: {msg}"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #3: A normal successful save_fn allows execution to proceed.
#[test]
fn tier_b_execute_succeeds_with_working_save_fn() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(
        &allowlist,
        "fixture/test",
        "tier-b-save-002",
        false,
        &clone_dest,
    )
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

    execute_tier_b(&plan, &mut manifest, &mut ctx)
        .value_or_panic("execution should succeed with working save_fn");

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #3: Save_fn failure on the second command (after first resource
/// is recorded) still propagates — no silent swallowing.
#[test]
fn tier_b_execute_propagates_save_fn_error_after_partial_success() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(
        &allowlist,
        "fixture/test",
        "tier-b-save-003",
        false,
        &clone_dest,
    )
    .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FakeTierBRunner::new();
    // Save succeeds the first time, then fails.
    let mut call_count = 0usize;
    let mut save_fn = |_m: &RunManifest| {
        call_count += 1;
        if call_count > 1 {
            Err("injected persistence failure on second save".to_string())
        } else {
            Ok(())
        }
    };
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let err = execute_tier_b(&plan, &mut manifest, &mut ctx)
        .err()
        .value_or_panic("second save_fn failure should propagate");
    assert!(
        err.to_string().contains("save manifest"),
        "error must mention save manifest: {err}"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// A runner that fails on any `create` command (simulating a gh failure).
struct FailingOnCreateRunner;

impl CommandRunner for FailingOnCreateRunner {
    fn run(
        &mut self,
        _program: &str,
        argv: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> Result<String, String> {
        if argv.contains(&"create".to_string()) {
            Err("simulated command failure".to_string())
        } else {
            Ok(String::new())
        }
    }
}

/// Finding #3 (remediation): When a command fails AND the failure-path save
/// also fails, execute_tier_b must propagate the save error rather than
/// silently swallowing it. The previous code used `let _ =` which discarded
/// save failures on the command-error path.
#[test]
fn tier_b_execute_propagates_save_fn_error_on_command_failure_path() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path();
    let clone_dest = run_root.join("fixture-clone");
    let plan = plan_tier_b(
        &allowlist,
        "fixture/test",
        "tier-b-save-004",
        false,
        &clone_dest,
    )
    .value_or_panic("plan");

    let mut manifest = make_manifest();
    let mut runner = FailingOnCreateRunner;
    // Save_fn that always fails — must propagate even on the error path.
    let mut save_fn = |_m: &RunManifest| Err("injected save failure on command error".to_string());
    let mut ctx = TierBContext {
        confirm_disposable: true,
        skip_validation: true,
        runner: &mut runner,
        clone_dest: &clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };

    let err = execute_tier_b(&plan, &mut manifest, &mut ctx)
        .err()
        .value_or_panic("save failure on command-error path must propagate");
    let msg = err.to_string();
    assert!(
        msg.contains("save manifest") || msg.contains("persistence"),
        "error must mention manifest save failure on command-error path: {msg}"
    );

    let _ = std::fs::remove_dir_all(&base);
}
