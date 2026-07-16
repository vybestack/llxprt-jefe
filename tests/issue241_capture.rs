//! Behavioral contracts for the bounded issue 241 tutorial capture workflow.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/issue241-capture.sh")
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    let status = Command::new("chmod")
        .args(["+x"])
        .arg(path)
        .status()
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
    assert!(status.success(), "chmod failed for {}", path.display());
}

fn fake_binaries(temp: &TempDir, capture_text: &str) -> (PathBuf, PathBuf) {
    let jefe = temp.path().join("jefe");
    write_executable(&jefe, "#!/bin/sh\nprintf 'jefe 0.0.29-test\\n'\n");
    let harness = temp.path().join("harness");
    let body = format!(
        "#!/bin/sh\nset -eu\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '--out-dir' ]; then out=$2; shift 2; else shift; fi\ndone\nmkdir -p \"$out\"\nfor name in first-agent-dashboard first-agent-new-repository first-agent-new-agent first-agent-terminal-ready first-agent-terminal-response first-agent-result; do\n  printf '%s\\n' '{}' > \"$out/$name.screen.txt\"\ndone\n",
        capture_text.replace('\'', "'\\''")
    );
    write_executable(&harness, &body);
    (jefe, harness)
}

fn capture(root: &Path, jefe: &Path, harness: &Path) -> Output {
    Command::new("sh")
        .arg(script())
        .args(["capture", "--root"])
        .arg(root)
        .arg("--jefe-bin")
        .arg(jefe)
        .arg("--harness-bin")
        .arg(harness)
        .output()
        .unwrap_or_else(|error| panic!("run capture: {error}"))
}

fn cleanup(root: &Path, mode: &str) -> Output {
    Command::new("sh")
        .arg(script())
        .args(["cleanup", mode, "--root"])
        .arg(root)
        .output()
        .unwrap_or_else(|error| panic!("run cleanup: {error}"))
}

#[test]
fn capture_refuses_relative_and_existing_roots() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(&temp, "publication-safe");
    let relative = capture(Path::new("relative-run"), &jefe, &harness);
    assert!(!relative.status.success());
    assert!(String::from_utf8_lossy(&relative.stderr).contains("absolute"));

    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap_or_else(|error| panic!("create existing root: {error}"));
    let output = capture(&existing, &jefe, &harness);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not exist"));
}

#[test]
fn successful_capture_records_provenance_and_renders_fixed_safe_svgs() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(
        &temp,
        "Tutorial Agent ready pid:123 [private-host 12:34 16-Jul-26",
    );
    let root = temp.path().join("capture");
    let output = capture(&root, &jefe, &harness);
    assert!(
        output.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read_to_string(root.join("manifest.txt"))
        .unwrap_or_else(|error| panic!("read manifest: {error}"));
    assert!(manifest.contains("outcome=success"));
    assert!(manifest.contains("jefe_version=jefe 0.0.29-test"));
    assert!(manifest.contains("jefe_commit="));
    let svg = fs::read_to_string(root.join("publication/first-agent-result.svg"))
        .unwrap_or_else(|error| panic!("read svg: {error}"));
    assert!(svg.contains("width=\"800\" height=\"576\""));
    assert!(svg.contains("Tutorial Agent ready"));
    assert!(svg.contains("pid:[redacted]"));
    assert!(svg.contains("[terminal status redacted]"));
    assert!(!svg.contains("pid:123"));
    assert!(!svg.contains("private-host"));
}

#[test]
fn unsafe_capture_fails_without_claiming_success_and_retains_diagnostics() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let username = std::env::var("USER").unwrap_or_else(|_| "capture-user".to_string());
    let (jefe, harness) = fake_binaries(&temp, &format!("private user: {username}"));
    let root = temp.path().join("unsafe");
    let output = capture(&root, &jefe, &harness);
    assert!(!output.status.success());

    let manifest = fs::read_to_string(root.join("manifest.txt"))
        .unwrap_or_else(|error| panic!("read failed manifest: {error}"));
    assert!(manifest.contains("outcome=failed"));
    assert!(!manifest.contains("outcome=success"));
    assert!(root.join("private/diagnostic.txt").is_file());
    assert!(
        root.join("evidence/first-agent-dashboard.screen.txt")
            .is_file()
    );
}

#[test]
fn cleanup_is_manifest_scoped_dry_run_first_and_preserves_evidence() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(&temp, "publication-safe");
    let root = temp.path().join("cleanup");
    let output = capture(&root, &jefe, &harness);
    assert!(output.status.success());
    fs::write(root.join("unowned.txt"), "preserve")
        .unwrap_or_else(|error| panic!("write unowned: {error}"));

    let dry_run = cleanup(&root, "--dry-run");
    assert!(dry_run.status.success());
    assert!(root.join("config").exists());
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("config"));

    let confirmed = cleanup(&root, "--confirm");
    assert!(confirmed.status.success());
    assert!(!root.join("config").exists());
    assert!(!root.join("fixture-repo").exists());
    assert!(root.join("evidence").is_dir());
    assert!(root.join("publication").is_dir());
    assert!(root.join("manifest.txt").is_file());
    assert!(root.join("unowned.txt").is_file());
}
