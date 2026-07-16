//! Behavioral contracts for the bounded issue 241 tutorial capture workflow.

#![cfg(unix)]
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

fn attribute_value<'a>(element: &'a str, name: &str) -> &'a str {
    let prefix = format!("{name}=\"");
    element
        .split_once(&prefix)
        .and_then(|(_, remainder)| remainder.split_once('"'))
        .map_or_else(
            || panic!("missing {name} attribute in {element}"),
            |(value, _)| value,
        )
}

fn numeric_attribute(element: &str, name: &str) -> u32 {
    let value = attribute_value(element, name);
    value
        .parse()
        .unwrap_or_else(|error| panic!("invalid {name} attribute {value}: {error}"))
}

fn rendered_text(row: &str) -> String {
    let content = row
        .split_once('>')
        .and_then(|(_, remainder)| remainder.split_once("</tspan>"))
        .map_or_else(
            || panic!("invalid tspan row: {row}"),
            |(content, _)| content,
        );
    content
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn assert_terminal_grid_is_contained(svg: &str, context: &str) {
    let root = svg
        .lines()
        .find(|line| line.starts_with("<svg "))
        .unwrap_or_else(|| panic!("missing SVG root in {context}"));
    let viewport_width = numeric_attribute(root, "width");
    let viewbox_width = attribute_value(root, "viewBox")
        .split_whitespace()
        .nth(2)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| panic!("invalid viewBox attribute in {context}"));
    assert_eq!(
        viewport_width, viewbox_width,
        "SVG width and viewBox width differ in {context}"
    );

    let mut row_count = 0;
    for row in svg.lines().filter(|line| line.starts_with("<tspan ")) {
        let text_x = numeric_attribute(row, "x");
        let text_length = numeric_attribute(row, "textLength");
        let text_end = text_x
            .checked_add(text_length)
            .unwrap_or_else(|| panic!("rendered terminal row overflows geometry in {context}"));
        assert!(
            text_end < viewbox_width,
            "rendered terminal row extends outside {context}"
        );
        let right_padding = viewport_width - text_end;
        assert_eq!(
            text_x, right_padding,
            "unequal horizontal padding in {context}"
        );
        assert_eq!(
            rendered_text(row).chars().count(),
            100,
            "rendered row must preserve 100 terminal columns in {context}"
        );
        row_count += 1;
    }
    assert!(row_count > 0, "missing rendered terminal rows in {context}");
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
fn capture_reports_a_missing_run_root_parent_before_creation() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(&temp, "publication-safe");
    let missing_parent = temp.path().join("missing-parent");
    let root = missing_parent.join("capture");
    let output = capture(&root, &jefe, &harness);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("run root parent directory does not exist")
    );
    assert!(!missing_parent.exists());
}

#[test]
fn capture_rejects_non_executable_binary_paths_before_creating_root() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let missing_jefe = temp.path().join("missing-jefe");
    let missing_harness = temp.path().join("missing-harness");
    let root = temp.path().join("invalid-binaries");
    let output = capture(&root, &missing_jefe, &missing_harness);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("jefe binary not found or not executable")
    );
    assert!(!root.exists());
}

#[test]
fn successful_capture_records_provenance_and_renders_fixed_safe_svgs() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(
        &temp,
        "pid:123\nTutorial Agent ready        pid:456\nProcess (pid:789) exited\n[private-host 12:34 16-Jul-26",
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
    let commit = manifest
        .lines()
        .find_map(|line| line.strip_prefix("jefe_commit="))
        .unwrap_or_else(|| panic!("manifest must record jefe_commit"));
    assert_eq!(commit.len(), 40, "expected a full git commit: {commit}");
    assert!(
        commit
            .chars()
            .all(|character| character.is_ascii_hexdigit()),
        "expected hexadecimal git commit: {commit}"
    );
    let svg = fs::read_to_string(root.join("publication/first-agent-result.svg"))
        .unwrap_or_else(|error| panic!("read svg: {error}"));
    assert!(svg.contains("width=\"880\" height=\"594\""));
    assert!(svg.contains("Tutorial Agent ready"));
    assert!(svg.contains("pid:[redacted]"));
    assert!(svg.contains("[terminal status redacted]"));
    assert!(!svg.contains("pid:123"));
    assert!(!svg.contains("pid:456"));
    assert!(!svg.contains("pid:789"));
    assert!(!svg.contains("private-host"));
    let publication = fs::read_to_string(root.join("private/first-agent-result.publication.txt"))
        .unwrap_or_else(|error| panic!("read publication text: {error}"));
    assert!(
        publication.lines().all(|line| line.chars().count() == 100),
        "publication rows must preserve the fixed 100-column capture grid"
    );
}

#[test]
fn renderer_contains_the_final_terminal_column_inside_fixed_padding() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let final_column = format!("{}│", " ".repeat(99));
    let (jefe, harness) = fake_binaries(&temp, &final_column);
    let root = temp.path().join("geometry");
    let output = capture(&root, &jefe, &harness);
    assert!(
        output.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for name in [
        "first-agent-dashboard",
        "first-agent-new-repository",
        "first-agent-new-agent",
        "first-agent-terminal-ready",
        "first-agent-terminal-response",
        "first-agent-result",
    ] {
        let path = root.join(format!("publication/{name}.svg"));
        let svg = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            svg.lines()
                .filter(|line| line.starts_with("<tspan "))
                .any(|line| rendered_text(line).trim_end().ends_with('│')),
            "final terminal column missing from {name}"
        );
        assert_terminal_grid_is_contained(&svg, name);
    }
}

#[test]
fn committed_tutorial_assets_keep_right_borders_inside_the_viewport() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/assets");
    for name in [
        "first-agent-new-repository.svg",
        "first-agent-new-agent.svg",
        "first-agent-result.svg",
    ] {
        let path = assets.join(name);
        let svg = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let rows = svg
            .lines()
            .filter(|line| line.starts_with("<tspan "))
            .map(rendered_text)
            .collect::<Vec<_>>();
        assert!(
            rows.iter()
                .any(|row| row.trim_end_matches([' ', '*']).ends_with('╮')),
            "right border missing from {name}"
        );
        assert!(
            rows.iter()
                .any(|row| row.trim_end_matches([' ', '*']).ends_with('│')),
            "right border missing from {name}"
        );
        assert_terminal_grid_is_contained(&svg, name);
    }
}

#[test]
#[should_panic(expected = "SVG width and viewBox width differ")]
fn geometry_contract_rejects_a_viewbox_narrower_than_the_canvas() {
    let row = "x".repeat(100);
    let svg = format!(
        "<svg width=\"880\" viewBox=\"0 0 800 594\">\n<tspan x=\"16\" textLength=\"848\">{row}</tspan>"
    );

    assert_terminal_grid_is_contained(&svg, "narrow viewBox fixture");
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
fn credential_validation_rejects_whitespace_around_delimiters() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let (jefe, harness) = fake_binaries(&temp, "password : exposed-value");
    let root = temp.path().join("credential");
    let output = capture(&root, &jefe, &harness);
    assert!(!output.status.success());

    let manifest = fs::read_to_string(root.join("manifest.txt"))
        .unwrap_or_else(|error| panic!("read failed manifest: {error}"));
    assert!(manifest.contains("outcome=failed"));
    assert!(manifest.contains("credential-like content"));
}

#[test]
fn cleanup_rejects_an_invalid_owned_path_after_a_valid_entry() {
    let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let root = temp.path().join("invalid-manifest");
    fs::create_dir_all(root.join("config"))
        .unwrap_or_else(|error| panic!("create config fixture: {error}"));
    fs::write(root.join(".issue241-run"), "jefe-issue241-capture-v1\n")
        .unwrap_or_else(|error| panic!("write sentinel: {error}"));
    fs::write(
        root.join("manifest.txt"),
        "format_version=1\nowned_path=config\nowned_path=outside\n",
    )
    .unwrap_or_else(|error| panic!("write manifest: {error}"));

    let output = cleanup(&root, "--confirm");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unrecognized owned path"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.join("config").is_dir(),
        "cleanup must validate every ownership entry before deleting anything"
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
