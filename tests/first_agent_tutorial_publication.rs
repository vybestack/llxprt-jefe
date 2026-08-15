use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
trait TestResult<T> {
    fn must(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> TestResult<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|err| panic!("{context}: {err}"))
    }
}

const PUBLISHED: [&str; 8] = [
    "first-agent-new-repository",
    "first-agent-new-agent",
    "first-agent-result",
    "first-agent-code-puppy",
    "first-agent-issues",
    "first-agent-issue-send",
    "first-agent-pull-request",
    "first-agent-pr-merge",
];

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn frame(markers: &[&str]) -> serde_json::Value {
    let mut lines: Vec<String> = markers.iter().map(|value| (*value).to_string()).collect();
    lines.push("Dir: /private/tmp/jefe-harness-1234-0-deadbeef…".to_string());
    lines.push("status:12:34:56 pid:12345".to_string());
    lines.resize(32, String::new());
    json!({"cols": 100, "rows": 32, "lines": lines})
}

fn report(extra_line: Option<&str>) -> serde_json::Value {
    let mut frames = vec![
        frame(&["Agent Types", "Code Puppy  Installed", "LLxprt  Installed"]),
        frame(&["New Repository", "LLxprt Jefe", "vybestack/llxprt-jefe"]),
        frame(&["New Agent", "Tutorial LLxprt", "core.llxprt"]),
        frame(&["tutorial-shim: ready", "LLxprt Jefe (1)"]),
        frame(&["tutorial-shim: response: hello from the", "LLxprt Jefe (1)"]),
        frame(&["New Agent", "Tutorial Puppy", "core.code-puppy"]),
        frame(&["#352", "OPEN", "labels: documentation, enhancement"]),
        frame(&["tutorial-shim: received issue 352", "LLxprt Jefe (2)"]),
        frame(&["#353", "decision: APPROVED", "OPEN"]),
        frame(&["Merge Pull Request #353", "Squash and merge"]),
    ];
    if let Some(line) = extra_line {
        frames[0]["lines"][3] = json!(line);
    }
    json!({
        "schema": 1,
        "status": "passed",
        "workspace": "/private/tmp/jefe-harness-1234-0-deadbeef",
        "frames": frames,
    })
}

fn run_publisher(report: &Path, root: &Path) -> std::process::Output {
    Command::new("python3")
        .arg(repo_path("scripts/publish-first-agent-tutorial.py"))
        .arg("--report")
        .arg(report)
        .arg("--root")
        .arg(root)
        .output()
        .must("run tutorial report publisher")
}

#[test]
fn canonical_report_produces_named_sanitized_evidence_and_publication_assets() {
    let temp = tempfile::tempdir().must("create temp directory");
    let report_path = temp.path().join("report.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report(None)).must("serialize report"),
    )
    .must("write report");

    let output = run_publisher(&report_path, temp.path());
    assert!(
        output.status.success(),
        "publisher failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let evidence = fs::read_dir(temp.path().join("evidence"))
        .must("read evidence")
        .count();
    assert_eq!(evidence, 11);
    for name in PUBLISHED {
        let text = fs::read_to_string(
            temp.path()
                .join("private")
                .join(format!("{name}.publication.txt")),
        )
        .must("read publication text");
        assert!(!text.contains("jefe-harness-"));
        assert!(!text.contains("pid:12345"));
        assert!(text.contains("pid:xxxxx"));

        let svg = fs::read_to_string(temp.path().join("publication").join(format!("{name}.svg")))
            .must("read publication SVG");
        assert!(svg.contains("width=\"848\" height=\"610\""));
        assert_eq!(svg.matches("<text ").count(), 32);
    }
}

#[test]
fn publication_fails_closed_for_credential_like_frame_text() {
    let temp = tempfile::tempdir().must("create temp directory");
    let report_path = temp.path().join("report.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report(Some("password=secret"))).must("serialize report"),
    )
    .must("write report");

    let output = run_publisher(&report_path, temp.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("credential-like text"));
}

#[test]
fn publication_fails_when_a_semantic_frame_is_absent() {
    let temp = tempfile::tempdir().must("create temp directory");
    let report_path = temp.path().join("report.json");
    let mut value = report(None);
    let Some(frames) = value["frames"].as_array_mut() else {
        panic!("frames must be an array");
    };
    frames.pop();
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&value).must("serialize report"),
    )
    .must("write report");

    let output = run_publisher(&report_path, temp.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no frame matching"));
}
