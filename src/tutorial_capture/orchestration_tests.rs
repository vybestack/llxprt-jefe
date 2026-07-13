use super::*;

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

/// Finding #5: Return the TempDir RAII guard so it is automatically cleaned
/// up when dropped, instead of calling `.keep()` which leaks the directory.
fn temp_base() -> tempfile::TempDir {
    tempfile::tempdir().value_or_panic("create temp base")
}

fn sample_setup(base: &Path) -> RunSetup {
    RunSetup {
        run_id: RunId::new("test-orch-001").value_or_panic("valid id"),
        base_dir: base.to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "test-scenario".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: Some("dark".to_string()),
        scenario_hash: None,
        shim_availability: path_shim::ShimAvailability::default(),
    }
}

// ── compute_directories ───────────────────────────────────────────────

fn compute_directories_layout() {
    let dirs = compute_directories(
        Path::new("/tmp/base"),
        &RunId::new("run-001").value_or_panic("valid id"),
    );
    assert_eq!(dirs.root, PathBuf::from("/tmp/base/run-001"));
    assert_eq!(dirs.config_dir, PathBuf::from("/tmp/base/run-001/config"));
    assert_eq!(
        dirs.artifact_dir,
        PathBuf::from("/tmp/base/run-001/artifacts")
    );
    assert_eq!(dirs.shim_dir, PathBuf::from("/tmp/base/run-001/shims"));
    assert_eq!(
        dirs.fixture_repo,
        PathBuf::from("/tmp/base/run-001/fixture-repo")
    );
}

fn manifest_path_is_in_root() {
    let dirs = compute_directories(
        Path::new("/tmp/base"),
        &RunId::new("run-001").value_or_panic("valid id"),
    );
    assert_eq!(
        dirs.manifest_path(),
        PathBuf::from("/tmp/base/run-001/run-manifest.json")
    );
}

fn report_path_is_in_artifacts() {
    let dirs = compute_directories(
        Path::new("/tmp/base"),
        &RunId::new("run-001").value_or_panic("valid id"),
    );
    assert_eq!(
        dirs.report_path(),
        PathBuf::from("/tmp/base/run-001/artifacts/run-report.md")
    );
}

// ── prepare_run ───────────────────────────────────────────────────────

fn prepare_run_creates_directory_tree() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.config_dir.exists());
    assert!(dirs.artifact_dir.exists());
    assert!(dirs.shim_dir.exists());
    assert!(dirs.fixture_repo.exists());
}

fn prepare_run_writes_shim_scripts_for_shim_profile() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.shim_dir.join("llxprt").exists());
    assert!(dirs.shim_dir.join("code-puppy").exists());
}

fn prepare_run_provisions_git_repo_with_commit() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.fixture_repo.join(".git").exists());
    assert!(dirs.fixture_repo.join("README.md").exists());
}

fn prepare_run_returns_manifest_with_owned_paths() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (_dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(!manifest.owned_paths.is_empty());
    assert_eq!(manifest.jefe_version, "0.0.28");
}

fn prepare_run_records_fixture_repo_in_manifest() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (_dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(manifest.fixture_repo_path.is_some());
}

fn prepare_run_persists_atomic_manifest() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(
        dirs.manifest_path().exists(),
        "manifest must be persisted immediately after prepare_run"
    );
}

/// Finding: shim_availability LlxprtOnly installs only the llxprt shim.
fn prepare_run_with_llxprt_only_installs_only_llxprt_shim() {
    let base = temp_base();
    let mut setup = sample_setup(base.path());
    setup.shim_availability = path_shim::ShimAvailability::LlxprtOnly;
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(
        dirs.shim_dir.join("llxprt").exists(),
        "llxprt shim must exist"
    );
    assert!(
        !dirs.shim_dir.join("code-puppy").exists(),
        "code-puppy shim must NOT exist for llxprt-only"
    );
}

/// Finding: shim_availability CodePuppyOnly installs only the code-puppy shim.
fn prepare_run_with_code_puppy_only_installs_only_code_puppy_shim() {
    let base = temp_base();
    let mut setup = sample_setup(base.path());
    setup.shim_availability = path_shim::ShimAvailability::CodePuppyOnly;
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(
        dirs.shim_dir.join("code-puppy").exists(),
        "code-puppy shim must exist"
    );
    assert!(
        !dirs.shim_dir.join("llxprt").exists(),
        "llxprt shim must NOT exist for code-puppy-only"
    );
}

fn prepare_run_fails_on_collision() {
    let base = temp_base();
    let setup = sample_setup(base.path());

    // First run succeeds.
    prepare_run(&setup).value_or_panic("first prepare_run");

    // Second run with same run_id should fail (collision).
    let err = error_or_panic(prepare_run(&setup), "should detect collision");
    assert!(
        matches!(
            err,
            OrchestrationError::Persistence(PersistenceError::RunRootCollision { .. })
        ),
        "should be RunRootCollision: {err:?}"
    );
}

fn shim_scripts_are_executable_on_unix() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(dirs.shim_dir.join("llxprt"))
            .value_or_panic("stat shim")
            .permissions()
            .mode();
        assert!(perms & 0o111 != 0, "shim must be executable");
    }
}

// ── save/load manifest ────────────────────────────────────────────────

fn save_and_load_manifest_roundtrip() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");
    manifest.set_outcome(super::super::manifest::RunOutcome::Success);
    save_manifest(&manifest, &dirs.manifest_path()).value_or_panic("save manifest");

    let loaded = load_manifest(&dirs.manifest_path()).value_or_panic("load manifest");
    assert_eq!(loaded.run_id, manifest.run_id);
    assert_eq!(loaded.outcome, super::super::manifest::RunOutcome::Success);
}

// ── check_fixture_repo ────────────────────────────────────────────────

fn check_fixture_repo_allows_allowlisted() {
    let allowlist = FixtureAllowlist::new(["fixture/test-repo"]);
    check_fixture_repo(&allowlist, "fixture/test-repo").value_or_panic("should allow");
}

fn check_fixture_repo_refuses_production() {
    let allowlist = FixtureAllowlist::new(["vybestack/jefe"]);
    let err = error_or_panic(
        check_fixture_repo(&allowlist, "vybestack/jefe"),
        "should refuse",
    );
    assert!(matches!(err, OrchestrationError::FixtureRefused { .. }));
}

fn check_fixture_repo_refuses_unlisted() {
    let allowlist = FixtureAllowlist::new(["fixture/test"]);
    let err = error_or_panic(
        check_fixture_repo(&allowlist, "other/repo"),
        "should refuse",
    );
    assert!(err.to_string().contains("not in the fixture allowlist"));
}

// ── cleanup ───────────────────────────────────────────────────────────

fn cleanup_removes_owned_paths() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.config_dir.exists());
    let records = cleanup_manifest(&mut manifest, true).value_or_panic("cleanup");

    assert!(!dirs.config_dir.exists());
    assert!(!dirs.shim_dir.exists());
    assert!(!dirs.fixture_repo.exists());
    assert!(!records.is_empty());
}

fn cleanup_marks_manifest_completed() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (_dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    cleanup_manifest(&mut manifest, true).value_or_panic("cleanup");
    assert!(manifest.cleanup_completed);
}

fn cleanup_skips_nonexistent_paths() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (_dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // Pre-remove the fixture repo to simulate partial state.

    cleanup_manifest(&mut manifest, true).value_or_panic("cleanup should skip missing paths");
}

// ── controlled_path ──────────────────────────────────────────────────

fn controlled_path_prepends_shim_dir() {
    let path = controlled_path_for(Path::new("/tmp/shims"));
    assert!(path.starts_with("/tmp/shims:"));
}

// ── detection_path_for: curated-only PATH projection (Finding #1) ─────

/// The curated PATH must be ONLY the shim directory — no inherited host PATH
/// entries — so the launched process cannot detect opposite-runtime binaries
/// from the host.
fn detection_path_for_returns_only_curated_bin() {
    let path = detection_path_for(Path::new("/tmp/run/shims"), RuntimeProfile::Shim);
    assert_eq!(
        path, "/tmp/run/shims",
        "detection_path_for must return ONLY the curated bin, not inherited PATH"
    );
}

fn detection_path_for_real_llxprt_returns_only_curated_bin() {
    let path = detection_path_for(Path::new("/tmp/run/shims"), RuntimeProfile::RealLlxprt);
    assert_eq!(path, "/tmp/run/shims");
}

// ── Curated bin contains selected runtime and system tools ───────────
//
// Finding #1: Behavioral tests that prepare a real run and then assert
// the curated bin (shim directory) contains the selected runtime shim/symlink
// and system tool symlinks, and does NOT contain the unselected runtime.

/// Shim profile + LlxprtOnly: curated bin contains llxprt shim, system tools,
/// and does NOT contain code-puppy shim.
fn curated_bin_shim_llxprt_only_has_llxprt_not_code_puppy() {
    let base = temp_base();
    let setup = RunSetup {
        run_id: RunId::new("curated-llxprt-only").value_or_panic("valid id"),
        base_dir: base.path().to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "test".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: None,
        scenario_hash: None,
        shim_availability: path_shim::ShimAvailability::LlxprtOnly,
    };
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // llxprt shim must be present.
    assert!(
        dirs.shim_dir.join("llxprt").exists(),
        "curated bin must contain llxprt shim for LlxprtOnly"
    );
    // code-puppy shim must be ABSENT.
    assert!(
        !dirs.shim_dir.join("code-puppy").exists(),
        "curated bin must NOT contain code-puppy shim for LlxprtOnly"
    );
    // System tools must be present (at least sh).
    assert!(
        dirs.shim_dir.join("sh").exists(),
        "curated bin must contain sh system tool symlink"
    );
}

/// Shim profile + CodePuppyOnly: curated bin contains code-puppy shim, system
/// tools, and does NOT contain llxprt shim.
fn curated_bin_shim_code_puppy_only_has_code_puppy_not_llxprt() {
    let base = temp_base();
    let setup = RunSetup {
        run_id: RunId::new("curated-cp-only").value_or_panic("valid id"),
        base_dir: base.path().to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "test".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: None,
        scenario_hash: None,
        shim_availability: path_shim::ShimAvailability::CodePuppyOnly,
    };
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(
        dirs.shim_dir.join("code-puppy").exists(),
        "curated bin must contain code-puppy shim for CodePuppyOnly"
    );
    assert!(
        !dirs.shim_dir.join("llxprt").exists(),
        "curated bin must NOT contain llxprt shim for CodePuppyOnly"
    );
    assert!(
        dirs.shim_dir.join("sh").exists(),
        "curated bin must contain sh system tool symlink"
    );
}

/// Shim profile + Both: curated bin contains both shims and system tools.
fn curated_bin_shim_both_has_both_runtimes() {
    let base = temp_base();
    let setup = RunSetup {
        run_id: RunId::new("curated-both").value_or_panic("valid id"),
        base_dir: base.path().to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "test".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: None,
        scenario_hash: None,
        shim_availability: path_shim::ShimAvailability::Both,
    };
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(
        dirs.shim_dir.join("llxprt").exists(),
        "curated bin must contain llxprt shim for Both"
    );
    assert!(
        dirs.shim_dir.join("code-puppy").exists(),
        "curated bin must contain code-puppy shim for Both"
    );
}

/// The curated bin must contain all required system tool symlinks so the
/// launched Jefe process can find git, tmux, sh, gh without the host PATH.
fn curated_bin_contains_required_system_tool_symlinks() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // sh must always be present (POSIX requirement).
    assert!(
        dirs.shim_dir.join("sh").exists(),
        "curated bin must contain sh symlink"
    );
    // git and tmux should be present if they exist on the host PATH.
    let inherited = std::env::var("PATH").unwrap_or_default();
    let system_links = path_shim::plan_system_tool_links(&inherited);
    for link in &system_links {
        assert!(
            dirs.shim_dir.join(&link.name).exists(),
            "curated bin must contain {} symlink (resolved to {})",
            link.name,
            link.target.display()
        );
    }
}

// ── save_report ───────────────────────────────────────────────────────

fn save_report_writes_markdown() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    save_report(&manifest, &dirs.report_path()).value_or_panic("save report");
    assert!(dirs.report_path().exists());

    let content = fs::read_to_string(dirs.report_path()).value_or_panic("read report");
    assert!(content.contains("Tutorial Capture Run Report"));
}

// ── git commit recording ─────────────────────────────────────────────

fn prepare_run_records_git_commit_when_available() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (_dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    if let Some(commit) = &manifest.git_commit {
        assert!(!commit.is_empty(), "git commit must not be empty string");
    }
}

// ── redact_artifacts ─────────────────────────────────────────────────

fn redact_artifacts_scrubs_full_token_values() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // Write a file containing a full GitHub token.
    let artifact = dirs.artifact_dir.join("test.screen.txt");
    let token = "ghp_abcdef1234567890ABCDEF1234567890";
    fs::write(&artifact, format!("token: {token}")).value_or_panic("write artifact");

    let count = redact_artifacts(&dirs.artifact_dir).value_or_panic("redact");
    assert!(count >= 1, "at least one file should have been redacted");

    let content = fs::read_to_string(&artifact).value_or_panic("read redacted");
    assert!(
        !content.contains("ghp_"),
        "token prefix must be redacted; got: {content}"
    );
    assert!(
        !content.contains("abcdef1234567890"),
        "token value must be fully redacted; got: {content}"
    );
    assert!(
        !content.contains(token),
        "original token must be absent; got: {content}"
    );
    assert!(
        content.contains("<token>"),
        "token must be replaced with placeholder; got: {content}"
    );
}

fn redact_artifacts_preserves_clean_files() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    let artifact = dirs.artifact_dir.join("clean.screen.txt");
    let original = "dashboard: LLxprt Jefe\nNo issues found";
    fs::write(&artifact, original).value_or_panic("write artifact");

    let count = redact_artifacts(&dirs.artifact_dir).value_or_panic("redact");
    assert_eq!(count, 0, "clean file should not be redacted");

    let content = fs::read_to_string(&artifact).value_or_panic("read back");
    assert_eq!(content, original);
}

fn redact_artifacts_recursively_scrubs_subdirectories() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // Create a sub-directory with a token-containing file.
    let subdir = dirs.artifact_dir.join("subdir");
    fs::create_dir_all(&subdir).value_or_panic("create subdir");
    let artifact = subdir.join("nested.screen.txt");
    fs::write(&artifact, "token: ghp_abcdef1234567890AAA11122233344")
        .value_or_panic("write artifact");

    let count = redact_artifacts(&dirs.artifact_dir).value_or_panic("redact");
    assert!(count >= 1, "nested file should be redacted");

    let content = fs::read_to_string(&artifact).value_or_panic("read redacted");
    assert!(
        !content.contains("ghp_"),
        "token must be redacted in nested files; got: {content}"
    );
}

// ─── Finding #5: symlink rejection in redaction ─────────────────────

/// Redaction must reject symlinks in the artifact directory (fail-closed).
/// A symlinked file could point outside the artifact directory, bypassing
/// the containment boundary.
#[cfg(unix)]
fn redact_artifacts_rejects_symlink_in_artifact_dir() {
    use std::os::unix::fs::symlink;
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // Create a real file outside the artifact dir, then symlink it in.
    let outside = base.path().join("outside-target.txt");
    fs::write(&outside, "token: ghp_abcdef1234567890AAA11122233344")
        .value_or_panic("write outside file");
    let link = dirs.artifact_dir.join("symlinked.screen.txt");
    symlink(&outside, &link).value_or_panic("create symlink");

    let err = error_or_panic(
        redact_artifacts(&dirs.artifact_dir),
        "should reject symlink",
    );
    assert!(
        err.to_string().contains("symlink"),
        "error must mention symlink: {err}"
    );
}

/// Redaction must reject symlinked subdirectories recursively.
#[cfg(unix)]
fn redact_artifacts_rejects_symlinked_subdirectory() {
    use std::os::unix::fs::symlink;
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    // Create a real directory outside the artifact dir, then symlink it in.
    let outside_dir = base.path().join("outside-dir");
    fs::create_dir_all(&outside_dir).value_or_panic("create outside dir");
    fs::write(
        outside_dir.join("nested.screen.txt"),
        "token: ghp_abcdef1234567890AAA11122233344",
    )
    .value_or_panic("write outside file");
    let link = dirs.artifact_dir.join("symlinked-subdir");
    symlink(&outside_dir, &link).value_or_panic("create symlinked subdir");

    let err = error_or_panic(
        redact_artifacts(&dirs.artifact_dir),
        "should reject symlinked subdir",
    );
    assert!(
        err.to_string().contains("symlink"),
        "error must mention symlink: {err}"
    );
}

// ── Finding #6: Redaction fail-closed: report via redaction, no private data ──

/// Finding #6: `save_report` must produce a report that does NOT contain
/// the actual user's home path, username, or private fixture repo names.
/// This verifies the report is redacted *before* it is written to disk.
fn save_report_redacts_private_data_and_paths() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let dirs = prepare_test_run(base.path(), RuntimeProfile::Shim);

    let manifest = sample_manifest_for_redaction(&dirs);
    let report_path = dirs.artifact_dir.join("run-report.md");

    save_report(&manifest, &report_path).value_or_panic("save_report should succeed");

    let report = std::fs::read_to_string(&report_path).value_or_panic("read report");
    // The private fixture repo name must be redacted to <repo>.
    assert!(
        !report.contains("fixture/secret-private-repo"),
        "report must not contain private fixture repo name: {report}"
    );
    assert!(
        report.contains("<repo>"),
        "report must contain <repo> placeholder for redacted repo: {report}"
    );
    // The actual home directory must not appear.
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();
    if !home.is_empty() {
        assert!(
            !report.contains(&home),
            "report must not contain actual home path '{home}': {report}",
        );
    }
}

/// Finding #6: `save_report` must not leak absolute home paths into the
/// published report. The report's owned_paths section contains paths that
/// include the home directory — these must be redacted to `~`.
fn save_report_no_absolute_home_path_in_published_report() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let dirs = prepare_test_run(base.path(), RuntimeProfile::Shim);
    let manifest = sample_manifest_for_redaction(&dirs);
    let report_path = dirs.artifact_dir.join("run-report.md");

    save_report(&manifest, &report_path).value_or_panic("save_report should succeed");

    let report = std::fs::read_to_string(&report_path).value_or_panic("read report");
    // The home directory must not appear literally in the report.
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();
    if !home.is_empty() {
        assert!(
            !report.contains(&home),
            "report must not contain absolute home path '{home}': {report}",
        );
    }
    // The private repo name must not appear.
    assert!(
        !report.contains("secret-private-repo"),
        "report must not contain private repo name: {report}"
    );
}

/// Helper: prepare a run and return its directories.
fn prepare_test_run(
    base: &Path,
    profile: RuntimeProfile,
) -> crate::tutorial_capture::RunDirectories {
    let run_id = RunId::new("redact-test").value_or_panic("valid id");
    let setup = RunSetup {
        run_id,
        base_dir: base.to_path_buf(),
        jefe_version: "0.0.28".to_string(),
        scenario_name: "test".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: profile,
        fixture_github_repo: Some("fixture/secret-private-repo".to_string()),
        jefe_bin: None,
        theme: Some("green-screen".to_string()),
        scenario_hash: None,
        shim_availability: crate::tutorial_capture::path_shim::ShimAvailability::default(),
    };
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");
    dirs
}

/// Helper: build a manifest with private data for redaction tests.
fn sample_manifest_for_redaction(dirs: &crate::tutorial_capture::RunDirectories) -> RunManifest {
    let run_id = RunId::new("redact-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(run_id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(OwnedPathKind::ConfigDir, dirs.config_dir.clone());
    manifest.add_owned_path(OwnedPathKind::ArtifactDir, dirs.artifact_dir.clone());
    manifest.set_fixture_github_repo("fixture/secret-private-repo");
    manifest.add_observed_action("S", "send to agent", Some("sent-checkpoint".to_string()));
    manifest.add_discrepancy("test discrepancy with /Users/private-user path");
    manifest
}

#[test]
fn orchestration_behaviors() {
    compute_directories_layout();
    manifest_path_is_in_root();
    report_path_is_in_artifacts();
    prepare_run_creates_directory_tree();
    prepare_run_writes_shim_scripts_for_shim_profile();
    prepare_run_provisions_git_repo_with_commit();
    prepare_run_returns_manifest_with_owned_paths();
    prepare_run_records_fixture_repo_in_manifest();
    prepare_run_persists_atomic_manifest();
    prepare_run_with_llxprt_only_installs_only_llxprt_shim();
    prepare_run_with_code_puppy_only_installs_only_code_puppy_shim();
    prepare_run_fails_on_collision();
    shim_scripts_are_executable_on_unix();
    save_and_load_manifest_roundtrip();
    check_fixture_repo_allows_allowlisted();
    check_fixture_repo_refuses_production();
    check_fixture_repo_refuses_unlisted();
    cleanup_removes_owned_paths();
    cleanup_marks_manifest_completed();
    cleanup_skips_nonexistent_paths();
    controlled_path_prepends_shim_dir();
    detection_path_for_returns_only_curated_bin();
    detection_path_for_real_llxprt_returns_only_curated_bin();
    curated_bin_shim_llxprt_only_has_llxprt_not_code_puppy();
    curated_bin_shim_code_puppy_only_has_code_puppy_not_llxprt();
    curated_bin_shim_both_has_both_runtimes();
    curated_bin_contains_required_system_tool_symlinks();
    save_report_writes_markdown();
    prepare_run_records_git_commit_when_available();
    redact_artifacts_scrubs_full_token_values();
    redact_artifacts_preserves_clean_files();
    redact_artifacts_recursively_scrubs_subdirectories();
    redact_artifacts_rejects_symlink_in_artifact_dir();
    redact_artifacts_rejects_symlinked_subdirectory();
    save_report_redacts_private_data_and_paths();
    save_report_no_absolute_home_path_in_published_report();
}
