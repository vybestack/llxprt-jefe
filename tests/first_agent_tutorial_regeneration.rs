//! Behavioral contracts for the supported first-agent tutorial regeneration.

#![cfg(unix)]

use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const ASSETS: [&str; 8] = [
    "first-agent-new-repository.svg",
    "first-agent-new-agent.svg",
    "first-agent-result.svg",
    "first-agent-code-puppy.svg",
    "first-agent-issues.svg",
    "first-agent-issue-send.svg",
    "first-agent-pull-request.svg",
    "first-agent-pr-merge.svg",
];

trait TestResult<T> {
    fn must(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> TestResult<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|err| panic!("{context}: {err}"))
    }
}

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
    binaries: [PathBuf; 5],
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().must("create temp directory");
        let repo = temp.path().join("repo");
        create_fixture_files(&repo);
        initialize_fixture_repository(&repo);
        let binaries = create_fake_binaries(temp.path());
        Self {
            _temp: temp,
            repo,
            binaries,
        }
    }

    fn root(&self, name: &str) -> PathBuf {
        match self.repo.parent() {
            Some(parent) => parent.join(name),
            None => panic!("fixture parent is absent"),
        }
    }

    fn regenerate(&self, root: &Path, environment: Option<(&str, &str)>) -> Output {
        let mut command = Command::new("sh");
        command
            .arg(self.repo.join("scripts/regenerate-first-agent-tutorial.sh"))
            .args(["regenerate", "--root"])
            .arg(root)
            .arg("--tmux-scenario")
            .arg(&self.binaries[0])
            .arg("--jefe")
            .arg(&self.binaries[1])
            .arg("--probe")
            .arg(&self.binaries[2])
            .arg("--jsp-fixture")
            .arg(&self.binaries[3])
            .arg("--shim")
            .arg(&self.binaries[4])
            .current_dir(&self.repo);
        if let Some((key, value)) = environment {
            command.env(key, value);
        }
        command.output().must("run regenerate")
    }

    fn check(&self) -> Output {
        Command::new("sh")
            .arg(self.repo.join("scripts/regenerate-first-agent-tutorial.sh"))
            .arg("check")
            .current_dir(&self.repo)
            .output()
            .must("run check")
    }

    fn cleanup(&self, mode: &str, root: &Path) -> Output {
        Command::new("sh")
            .arg(self.repo.join("scripts/regenerate-first-agent-tutorial.sh"))
            .args(["cleanup", mode, "--root"])
            .arg(root)
            .current_dir(&self.repo)
            .output()
            .must("run cleanup")
    }
}

fn create_fixture_files(repo: &Path) {
    for directory in [
        "scripts",
        "src/harness/v1",
        "src/bin",
        "docs/assets",
        "dev-docs/testing",
        "dev-docs/tmux-scenarios",
    ] {
        fs::create_dir_all(repo.join(directory)).must("create fixture directory");
    }
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/regenerate-first-agent-tutorial.sh"),
        repo.join("scripts/regenerate-first-agent-tutorial.sh"),
    )
    .must("copy regeneration script");
    create_script_fixtures(repo);
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/harness/v1/mod.rs",
        "src/bin/tmux_scenario.rs",
        "src/bin/jefe-capture-shim.rs",
        "dev-docs/testing/scenario-execution-manifest.json",
        "dev-docs/tmux-scenarios/first-agent-tutorial.json",
    ] {
        fs::write(repo.join(path), format!("fixture {path}\n")).must("write source fixture");
    }
    for asset in ASSETS {
        fs::write(repo.join("docs/assets").join(asset), "old\n").must("write old asset");
    }
    fs::write(
        repo.join("docs/assets/first-agent-tutorial.provenance"),
        "old\n",
    )
    .must("write old provenance");
}

fn create_script_fixtures(repo: &Path) {
    write_executable(
        &repo.join("scripts/run-scenario-manifest.py"),
        r#"#!/bin/sh
reports=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--reports" ]; then reports=$2; shift 2; else shift; fi
done
[ -n "$reports" ] || exit 2
mkdir -p "$reports"
printf '{}\n' > "$reports/dev-docs__tmux-scenarios__first-agent-tutorial.json"
printf '%s\n' "$*" > "$reports/driver-args.txt"
"#,
    );
    let mut publisher = String::from(
        r#"#!/bin/sh
root=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--root" ]; then root=$2; shift 2; else shift; fi
done
[ -n "$root" ] || exit 2
[ "${OMIT_PRIVATE-0}" = 1 ] || mkdir -p "$root/private"
mkdir -p "$root/publication" "$root/evidence"
"#,
    );
    for asset in ASSETS {
        writeln!(
            publisher,
            "[ \"${{OMIT_ASSET-}}\" = \"{asset}\" ] || printf '<svg>{asset}</svg>\\n' > \"$root/publication/{asset}\""
        )
        .must("append publisher fixture");
    }
    write_executable(
        &repo.join("scripts/publish-first-agent-tutorial.py"),
        &publisher,
    );
}

fn initialize_fixture_repository(repo: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Fixture User"],
        &["config", "user.email", "fixture@example.invalid"],
        &["add", "."],
        &["commit", "-qm", "fixture"],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .must("run git fixture command");
        assert!(output.status.success());
    }
}

fn create_fake_binaries(root: &Path) -> [PathBuf; 5] {
    let paths = [
        root.join("tmux_scenario"),
        root.join("jefe"),
        root.join("probe"),
        root.join("jsp-fixture"),
        root.join("capture-shim"),
    ];
    for path in &paths {
        write_executable(path, "#!/bin/sh\nprintf 'jefe 9.9.9-fixture\\n'\n");
    }
    paths
}

fn write_executable(path: &Path, body: &str) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(path)
        .must("create executable fixture");
    file.write_all(body.as_bytes())
        .must("write executable fixture");
}

fn diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_original_assets(fixture: &Fixture) {
    for asset in ASSETS {
        assert_eq!(
            fs::read_to_string(fixture.repo.join("docs/assets").join(asset))
                .must("read original asset"),
            "old\n"
        );
    }
}

#[test]
fn regeneration_uses_manifest_driver_and_records_verifiable_provenance() {
    let fixture = Fixture::new();
    let root = fixture.root("successful-run");
    let output = fixture.regenerate(&root, None);
    assert!(output.status.success(), "{}", diagnostics(&output));

    for asset in ASSETS {
        assert_eq!(
            fs::read_to_string(fixture.repo.join("docs/assets").join(asset))
                .must("read promoted asset"),
            format!("<svg>{asset}</svg>\n")
        );
    }
    let provenance = fs::read_to_string(
        fixture
            .repo
            .join("docs/assets/first-agent-tutorial.provenance"),
    )
    .must("read provenance");
    assert!(provenance.contains("format_version=2"));
    assert!(provenance.contains("source_version=jefe 9.9.9-fixture"));
    for asset in ASSETS {
        assert!(provenance.contains(&format!("asset={asset}:")));
    }
    let manifest = fs::read_to_string(root.join("manifest.txt")).must("read run manifest");
    assert!(manifest.contains("runner=tmux_scenario"));
    assert!(!manifest.contains("harness"));
    let check = fixture.check();
    assert!(check.status.success(), "{}", diagnostics(&check));
}

#[test]
fn regeneration_rejects_relative_or_existing_roots() {
    let fixture = Fixture::new();
    let relative = fixture.regenerate(Path::new("relative-run"), None);
    assert!(!relative.status.success());
    assert!(String::from_utf8_lossy(&relative.stderr).contains("absolute --root"));

    let root = fixture.root("existing");
    fs::create_dir(&root).must("create existing root");
    let existing = fixture.regenerate(&root, None);
    assert!(!existing.status.success());
    assert!(String::from_utf8_lossy(&existing.stderr).contains("must not already exist"));
}

#[test]
fn incomplete_publication_does_not_replace_committed_assets() {
    let fixture = Fixture::new();
    let root = fixture.root("incomplete");
    let output = fixture.regenerate(&root, Some(("OMIT_ASSET", "first-agent-result.svg")));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing publication asset"));
    assert_original_assets(&fixture);
}

#[test]
fn concurrent_promotion_owner_is_refused() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.repo.join("docs/assets/.first-agent-tutorial.lock"))
        .must("create promotion lock");
    let output = fixture.regenerate(&fixture.root("locked"), None);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("another regeneration owns promotion")
    );
    assert_original_assets(&fixture);
}

#[test]
fn check_detects_source_and_asset_mutations() {
    let source_fixture = Fixture::new();
    let output = source_fixture.regenerate(&source_fixture.root("generated"), None);
    assert!(output.status.success(), "{}", diagnostics(&output));
    fs::write(
        source_fixture.repo.join("src/harness/v1/mod.rs"),
        "changed\n",
    )
    .must("change source");
    let source_check = source_fixture.check();
    assert!(!source_check.status.success());
    assert!(String::from_utf8_lossy(&source_check.stderr).contains("source fingerprint is stale"));

    let asset_fixture = Fixture::new();
    let output = asset_fixture.regenerate(&asset_fixture.root("generated"), None);
    assert!(output.status.success(), "{}", diagnostics(&output));
    fs::write(
        asset_fixture
            .repo
            .join("docs/assets/first-agent-result.svg"),
        "changed\n",
    )
    .must("change asset");
    let asset_check = asset_fixture.check();
    assert!(!asset_check.status.success());
    assert!(String::from_utf8_lossy(&asset_check.stderr).contains("asset is stale"));
}

#[test]
fn cleanup_requires_the_owned_sentinel_and_explicit_confirmation() {
    let fixture = Fixture::new();
    let root = fixture.root("cleanup");
    let output = fixture.regenerate(&root, None);
    assert!(output.status.success(), "{}", diagnostics(&output));

    let preview = fixture.cleanup("--dry-run", &root);
    assert!(preview.status.success());
    assert!(root.exists());
    let confirmed = fixture.cleanup("--confirm", &root);
    assert!(confirmed.status.success());
    assert!(!root.exists());

    let foreign = fixture.root("foreign");
    fs::create_dir(&foreign).must("create foreign root");
    let refused = fixture.cleanup("--confirm", &foreign);
    assert!(!refused.status.success());
    assert!(foreign.exists());
}
