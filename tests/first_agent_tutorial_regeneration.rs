//! Behavioral contracts for the supported first-agent tutorial regeneration.

#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const ASSETS: [&str; 3] = [
    "first-agent-new-repository.svg",
    "first-agent-new-agent.svg",
    "first-agent-result.svg",
];

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
    jefe: PathBuf,
    harness: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let repo = temp.path().join("repo");
        create_fixture_files(&repo);
        initialize_fixture_repository(&repo);
        let (jefe, harness) = create_fake_binaries(temp.path());
        Self {
            _temp: temp,
            repo,
            jefe,
            harness,
        }
    }

    fn regenerate(&self, root_name: &str) -> Output {
        self.regenerate_with_env(root_name, None)
    }

    fn regenerate_with_env(&self, root_name: &str, environment: Option<(&str, &str)>) -> Output {
        self.regenerate_at(
            &self
                .repo
                .parent()
                .unwrap_or_else(|| panic!("fixture parent"))
                .join(root_name),
            environment,
        )
    }

    fn regenerate_at(&self, root: &Path, environment: Option<(&str, &str)>) -> Output {
        let mut command = Command::new("sh");
        command
            .arg(self.repo.join("scripts/regenerate-first-agent-tutorial.sh"))
            .args(["regenerate", "--root"])
            .arg(root)
            .arg("--jefe-bin")
            .arg(&self.jefe)
            .arg("--harness-bin")
            .arg(&self.harness)
            .current_dir(&self.repo);
        if let Some((key, value)) = environment {
            command.env(key, value);
        }
        command
            .output()
            .unwrap_or_else(|error| panic!("run regenerate: {error}"))
    }

    fn check(&self) -> Output {
        Command::new("sh")
            .arg(self.repo.join("scripts/regenerate-first-agent-tutorial.sh"))
            .arg("check")
            .current_dir(&self.repo)
            .output()
            .unwrap_or_else(|error| panic!("run check: {error}"))
    }
}

fn create_fixture_files(repo: &Path) {
    for directory in ["scripts", "src", "docs/assets", "dev-docs/tmux-scenarios"] {
        fs::create_dir_all(repo.join(directory))
            .unwrap_or_else(|error| panic!("create {directory}: {error}"));
    }
    let source_script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/regenerate-first-agent-tutorial.sh");
    fs::copy(
        source_script,
        repo.join("scripts/regenerate-first-agent-tutorial.sh"),
    )
    .unwrap_or_else(|error| panic!("copy regeneration script: {error}"));
    write_executable(
        &repo.join("scripts/issue241-capture.sh"),
        include_str!("fixtures/first_agent_tutorial/fake-capture.sh"),
    );
    write_source_contract(repo);
    for asset in ASSETS {
        fs::write(repo.join("docs/assets").join(asset), "old\n")
            .unwrap_or_else(|error| panic!("write old {asset}: {error}"));
    }
    fs::write(
        repo.join("docs/assets/first-agent-tutorial.provenance"),
        "old\n",
    )
    .unwrap_or_else(|error| panic!("write old provenance: {error}"));
}

fn write_source_contract(repo: &Path) {
    for (path, body) in [
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
        ),
        ("Cargo.lock", "# fixture\n"),
        ("build.rs", "fn main() {}\n"),
        ("src/lib.rs", "pub fn fixture() {}\n"),
        (
            "dev-docs/tmux-scenarios/first-agent-tutorial.json",
            "{\"steps\":[]}\n",
        ),
    ] {
        fs::write(repo.join(path), body).unwrap_or_else(|error| panic!("write {path}: {error}"));
    }
}

fn initialize_fixture_repository(repo: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Fixture User"],
        &["config", "user.email", "fixture@example.invalid"],
        &["add", "."],
        &["commit", "-qm", "fixture"],
    ] {
        run_success(Command::new("git").args(args).current_dir(repo));
    }
}

fn create_fake_binaries(root: &Path) -> (PathBuf, PathBuf) {
    let jefe = root.join("jefe");
    let harness = root.join("harness");
    write_executable(&jefe, "#!/bin/sh\nprintf 'jefe 9.9.9-fixture\\n'\n");
    write_executable(&harness, "#!/bin/sh\nexit 0\n");
    (jefe, harness)
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    let mut permissions = fs::metadata(path)
        .unwrap_or_else(|error| panic!("read permissions for {}: {error}", path.display()))
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .unwrap_or_else(|error| panic!("set permissions for {}: {error}", path.display()));
}

fn output_diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run_success(command: &mut Command) {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("run fixture command: {error}"));
    assert!(
        output.status.success(),
        "fixture command failed:\n{}",
        output_diagnostics(&output)
    );
}

#[test]
fn regeneration_promotes_only_selected_assets_and_records_provenance() {
    let fixture = Fixture::new();
    let output = fixture.regenerate("successful-run");
    assert!(
        output.status.success(),
        "regeneration failed:\n{}",
        output_diagnostics(&output)
    );

    for asset in ASSETS {
        let promoted = fs::read_to_string(fixture.repo.join("docs/assets").join(asset))
            .unwrap_or_else(|error| panic!("read promoted {asset}: {error}"));
        assert_eq!(promoted, format!("<svg>{asset}</svg>\n"));
    }
    let provenance = fs::read_to_string(
        fixture
            .repo
            .join("docs/assets/first-agent-tutorial.provenance"),
    )
    .unwrap_or_else(|error| panic!("read provenance: {error}"));
    assert!(provenance.contains("format_version=1"));
    assert!(provenance.contains("source_version=jefe 9.9.9-fixture"));
    assert!(provenance.contains("source_commit="));
    assert!(provenance.contains("source_fingerprint="));
    for asset in ASSETS {
        assert!(provenance.contains(&format!("asset={asset}:")));
    }
    let check = fixture.check();
    assert!(
        check.status.success(),
        "fresh assets should verify:\n{}",
        output_diagnostics(&check)
    );
}

#[test]
fn regeneration_rejects_relative_root_before_delegating_to_capture() {
    let fixture = Fixture::new();
    let output = fixture.regenerate_at(Path::new("relative-run"), None);

    assert!(
        !output.status.success(),
        "relative root unexpectedly succeeded:\n{}",
        output_diagnostics(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("absolute --root"),
        "absolute-root diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
    assert!(!fixture.repo.join("relative-run").exists());
}

#[test]
fn regeneration_reports_a_missing_flag_value() {
    let fixture = Fixture::new();
    let output = Command::new("sh")
        .arg(
            fixture
                .repo
                .join("scripts/regenerate-first-agent-tutorial.sh"),
        )
        .args(["regenerate", "--root"])
        .current_dir(&fixture.repo)
        .output()
        .unwrap_or_else(|error| panic!("run missing-value regeneration: {error}"));

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--root requires a value"),
        "missing-value diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
}

#[test]
fn regeneration_reports_a_missing_capture_private_directory() {
    let fixture = Fixture::new();
    let output = fixture.regenerate_with_env("missing-private-run", Some(("OMIT_PRIVATE", "1")));

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("capture private directory is missing or unsafe"),
        "missing-private diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
    for asset in ASSETS {
        let contents = fs::read_to_string(fixture.repo.join("docs/assets").join(asset))
            .unwrap_or_else(|error| panic!("read original {asset}: {error}"));
        assert_eq!(contents, "old\n", "{asset} must not be replaced");
    }
}

#[test]
fn regeneration_refuses_incomplete_publication_before_replacing_assets() {
    let fixture = Fixture::new();
    let output = fixture.regenerate_with_env(
        "incomplete-run",
        Some(("OMIT_ASSET", "first-agent-result.svg")),
    );

    assert!(
        !output.status.success(),
        "incomplete publication unexpectedly succeeded:\n{}",
        output_diagnostics(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("missing publication asset"),
        "missing-asset diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
    assert_original_assets(&fixture);
}

#[test]
fn regeneration_refuses_a_concurrent_promotion_owner() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.repo.join("docs/assets/.first-agent-tutorial.lock"))
        .unwrap_or_else(|error| panic!("create promotion lock: {error}"));

    let output = fixture.regenerate("locked-run");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("another regeneration owns promotion"),
        "lock diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
    assert_original_assets(&fixture);
}

#[test]
fn promotion_failure_restores_every_committed_asset() {
    let fixture = Fixture::new();
    let fake_bin = fixture.repo.join("fake-bin");
    fs::create_dir(&fake_bin).unwrap_or_else(|error| panic!("create fake bin: {error}"));
    write_executable(
        &fake_bin.join("mv"),
        "#!/bin/sh\ntarget=\nfor arg in \"$@\"; do target=$arg; done\ncase \"$target\" in */first-agent-new-agent.svg) exit 73 ;; esac\nexec /bin/mv \"$@\"\n",
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = fixture.regenerate_with_env("failed-promotion-run", Some(("PATH", &path)));

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("promotion failed; restored"),
        "rollback diagnostic was absent:\n{}",
        output_diagnostics(&output)
    );
    assert_original_assets(&fixture);
}

fn assert_original_assets(fixture: &Fixture) {
    for asset in ASSETS {
        let contents = fs::read_to_string(fixture.repo.join("docs/assets").join(asset))
            .unwrap_or_else(|error| panic!("read original {asset}: {error}"));
        assert_eq!(contents, "old\n", "{asset} must not be replaced");
    }
}

#[test]
fn check_detects_stale_source_contract_and_promoted_asset_bytes() {
    let source_fixture = Fixture::new();
    let generated = source_fixture.regenerate("source-stale-run");
    assert!(
        generated.status.success(),
        "source fixture regeneration failed:\n{}",
        output_diagnostics(&generated)
    );
    fs::write(
        source_fixture.repo.join("src/lib.rs"),
        "pub fn changed() {}\n",
    )
    .unwrap_or_else(|error| panic!("change source: {error}"));
    let source_check = source_fixture.check();
    assert!(
        !source_check.status.success(),
        "stale source unexpectedly verified:\n{}",
        output_diagnostics(&source_check)
    );
    assert!(
        String::from_utf8_lossy(&source_check.stderr).contains("source fingerprint is stale"),
        "stale-source diagnostic was absent:\n{}",
        output_diagnostics(&source_check)
    );

    let asset_fixture = Fixture::new();
    let generated = asset_fixture.regenerate("asset-stale-run");
    assert!(
        generated.status.success(),
        "asset fixture regeneration failed:\n{}",
        output_diagnostics(&generated)
    );
    fs::write(
        asset_fixture
            .repo
            .join("docs/assets/first-agent-result.svg"),
        "changed\n",
    )
    .unwrap_or_else(|error| panic!("change asset: {error}"));
    let asset_check = asset_fixture.check();
    assert!(
        !asset_check.status.success(),
        "stale asset unexpectedly verified:\n{}",
        output_diagnostics(&asset_check)
    );
    assert!(
        String::from_utf8_lossy(&asset_check.stderr).contains("asset is stale"),
        "stale-asset diagnostic was absent:\n{}",
        output_diagnostics(&asset_check)
    );
}
