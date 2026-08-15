use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use super::*;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jefe-scenario-manifest-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("create temporary directory {}: {err}", path.display()));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn driver_rejects_explicit_selection_combined_with_sharding() {
    let temporary = TempDir::new("selection-shard");
    let reports = temporary.path().join("reports");
    let output = execution_command("linux", host_true(), &reports)
        .args([
            "--scenario",
            "dev-docs/tmux-scenarios/issue382/agent-fresh-issue.json",
            "--shard-index",
            "0",
            "--shard-count",
            "2",
        ])
        .output()
        .unwrap_or_else(|err| panic!("run manifest driver: {err}"));
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("explicit scenario selection cannot be combined with sharding")
    );
    assert!(!reports.exists());
}

#[test]
fn driver_rejects_malformed_manifest_shapes_without_a_traceback() {
    let temporary = TempDir::new("malformed-manifest");
    let scripts = temporary.path().join("scripts");
    let evidence = temporary.path().join("dev-docs/testing");
    fs::create_dir(&scripts).must("driver test setup");
    fs::create_dir_all(&evidence).must("driver test setup");
    fs::copy(
        repo_path("scripts/run-scenario-manifest.py"),
        scripts.join("run-scenario-manifest.py"),
    )
    .must("driver test setup");
    assert_manifest_load_fails(
        &scripts,
        &evidence,
        &json!({"schema": 1, "scenarios": [null]}),
        "manifest scenario 0 shape differs",
    );

    let mut invalid_timeout: Value =
        serde_json::from_str(&read_repo_text(MANIFEST_PATH)).must("driver test setup");
    invalid_timeout["scenarios"][0]["timeout_ms"] = json!(true);
    assert_manifest_load_fails(
        &scripts,
        &evidence,
        &invalid_timeout,
        "timeout must be a positive integer",
    );

    let base: Value =
        serde_json::from_str(&read_repo_text(MANIFEST_PATH)).must("driver test setup");
    let overlong = "a".repeat(65);
    for unsafe_name in [
        "../escape",
        "a/b",
        r"a\b",
        "/absolute",
        ".",
        "..",
        &overlong,
    ] {
        let mut manifest = base.clone();
        manifest["scenarios"][0]["command"]["installs"][0]["name"] = json!(unsafe_name);
        assert_manifest_load_fails(&scripts, &evidence, &manifest, "install name is invalid");
    }

    let mut duplicate = base;
    let name = duplicate["scenarios"][0]["command"]["installs"][0]["name"].clone();
    duplicate["scenarios"][0]["command"]["installs"][1]["name"] = name;
    assert_manifest_load_fails(
        &scripts,
        &evidence,
        &duplicate,
        "install names must be sorted and unique",
    );
}

fn assert_manifest_load_fails(scripts: &Path, evidence: &Path, manifest: &Value, expected: &str) {
    fs::write(
        evidence.join("scenario-execution-manifest.json"),
        serde_json::to_vec(manifest).must("driver test setup"),
    )
    .must("driver test setup");
    let output = Command::new("python3")
        .arg(scripts.join("run-scenario-manifest.py"))
        .args(["--platform", "linux"])
        .output()
        .must("driver test setup");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains(expected),
        "expected {expected:?}, stderr={stderr}"
    );
    assert!(!stderr.contains("Traceback"), "stderr={stderr}");
}

#[test]
fn driver_verifies_exact_shard_union_and_report_inventory() {
    let temporary = TempDir::new("completion");
    let manifest = load_manifest();
    let required: Vec<&ScenarioEntry> = manifest
        .scenarios
        .iter()
        .filter(|entry| entry.platforms["linux"].disposition == "required")
        .collect();
    for shard_index in 0..2 {
        write_synthetic_shard(temporary.path(), &required, shard_index, 2);
    }

    let valid = verification_command(temporary.path())
        .output()
        .must("driver test setup");
    assert_command_passed(&valid);
    assert_mutated_report_is_rejected(temporary.path(), &required[0].path);
    assert_extra_report_is_rejected(temporary.path());
    assert_missing_report_is_rejected(temporary.path(), &required[0].path);
}

fn assert_command_passed(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_mutated_report_is_rejected(root: &Path, path: &str) {
    let report = root.join("shard-0").join(driver_report_name(path));
    let original = fs::read(&report).must("driver test setup");
    let mut changed: Value = serde_json::from_slice(&original).must("driver test setup");
    changed["steps"][0]["status"] = json!("failed");
    fs::write(
        &report,
        serde_json::to_vec_pretty(&changed).must("driver test setup"),
    )
    .must("driver test setup");
    let result = verification_command(root)
        .output()
        .must("driver test setup");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("report step 0 must be an error-free pass")
    );
    fs::write(report, original).must("driver test setup");
}

fn assert_extra_report_is_rejected(root: &Path) {
    let extra = root.join("extra.json");
    fs::write(&extra, "{}\n").must("driver test setup");
    let result = verification_command(root)
        .output()
        .must("driver test setup");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("completion report file inventory differs")
    );
    fs::remove_file(extra).must("driver test setup");
}

fn assert_missing_report_is_rejected(root: &Path, path: &str) {
    let missing = root.join("shard-0").join(driver_report_name(path));
    fs::remove_file(missing).must("driver test setup");
    let result = verification_command(root)
        .output()
        .must("driver test setup");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("No such file"));
}

#[test]
fn driver_timeout_terminates_its_process_group_and_aborts_the_shard() {
    let temporary = TempDir::new("timeout");
    let (runner, marker) = write_timeout_fixture(temporary.path());
    let reports = temporary.path().join("reports");
    let output = timeout_command(temporary.path(), &runner, &reports)
        .output()
        .must("driver timeout setup");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("exceeded its outer timeout"),
        "stderr={stderr}"
    );
    assert_eq!(
        fs::read_to_string(marker).must("driver timeout marker"),
        "terminated"
    );
    assert!(!reports.join("_completion.json").exists());
}

fn write_timeout_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let scripts = root.join("scripts");
    let evidence = root.join("dev-docs/testing");
    let scenarios = root.join("dev-docs/tmux-scenarios");
    fs::create_dir(&scripts).must("driver timeout setup");
    fs::create_dir_all(&evidence).must("driver timeout setup");
    fs::create_dir_all(&scenarios).must("driver timeout setup");
    fs::copy(
        repo_path("scripts/run-scenario-manifest.py"),
        scripts.join("run-scenario-manifest.py"),
    )
    .must("driver timeout setup");
    fs::write(
        scenarios.join("timeout-fixture.json"),
        serde_json::to_vec(&json!({"steps": [{"op": "finish"}]})).must("driver timeout setup"),
    )
    .must("driver timeout setup");
    fs::write(
        evidence.join("scenario-execution-manifest.json"),
        serde_json::to_vec(&timeout_manifest()).must("driver timeout setup"),
    )
    .must("driver timeout setup");
    let marker = root.join("terminated");
    let runner = root.join("runner");
    write_timeout_runner(&runner, &marker);
    (runner, marker)
}

fn timeout_manifest() -> Value {
    json!({
        "schema": 1,
        "scenarios": [{
            "path": "dev-docs/tmux-scenarios/timeout-fixture.json",
            "scenario_schema": 1,
            "criteria": ["CW00B-02"],
            "platforms": {
                "linux": {"disposition": "unsupported", "reason": "fixture"},
                "macos": {"disposition": "required"},
                "windows": {"disposition": "unsupported", "reason": "fixture"}
            },
            "command": {"binary": "tmux_scenario", "installs": []},
            "timeout_ms": 500,
            "ci_job": "tui_scenarios_macos",
            "expect": {
                "exit_code": 0,
                "report_status": "passed",
                "steps_total": 1,
                "operations": ["finish"],
                "assertions": {},
                "captures": 0,
                "capture_names": [],
                "failed_step": null
            }
        }]
    })
}

fn write_timeout_runner(runner: &Path, marker: &Path) {
    fs::write(
        runner,
        format!(
            "#!/usr/bin/env python3\nimport pathlib\nimport signal\n\ndef terminate(_signal, _frame):\n    pathlib.Path({marker:?}).write_text('terminated')\n    raise SystemExit(143)\n\nsignal.signal(signal.SIGTERM, terminate)\nsignal.pause()\n",
            marker = marker.to_string_lossy()
        ),
    )
    .must("driver timeout setup");
    fs::set_permissions(runner, fs::Permissions::from_mode(0o755)).must("driver timeout setup");
}

fn timeout_command(root: &Path, runner: &Path, reports: &Path) -> Command {
    let mut command = Command::new("python3");
    let true_path = host_true();
    command
        .arg(root.join("scripts/run-scenario-manifest.py"))
        .args(["--platform", "macos", "--tmux-scenario"])
        .arg(runner)
        .arg("--jefe")
        .arg(true_path)
        .arg("--probe")
        .arg(true_path)
        .arg("--jsp-fixture")
        .arg(true_path)
        .arg("--shim")
        .arg(true_path)
        .arg("--reports")
        .arg(reports);
    command
}

fn execution_command(platform: &str, runner: &Path, reports: &Path) -> Command {
    let mut command = Command::new("python3");
    let true_path = host_true();
    command
        .arg(repo_path("scripts/run-scenario-manifest.py"))
        .args(["--platform", platform, "--tmux-scenario"])
        .arg(runner)
        .arg("--jefe")
        .arg(true_path)
        .arg("--probe")
        .arg(true_path)
        .arg("--jsp-fixture")
        .arg(true_path)
        .arg("--shim")
        .arg(true_path)
        .arg("--reports")
        .arg(reports);
    command
}

fn host_true() -> &'static Path {
    ["/usr/bin/true", "/bin/true"]
        .into_iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("true executable is unavailable"))
}

fn verification_command(root: &Path) -> Command {
    let mut command = Command::new("python3");
    command
        .arg(repo_path("scripts/run-scenario-manifest.py"))
        .args(["--platform", "linux", "--verify-completion"])
        .arg(root)
        .args(["--expected-shards", "2"]);
    command
}

fn write_synthetic_report(directory: &Path, entry: &ScenarioEntry) {
    let scenario: Value = serde_json::from_str(&read_repo_text(&entry.path))
        .unwrap_or_else(|err| panic!("parse {}: {err}", entry.path));
    let operations = scenario["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("{} steps must be an array", entry.path));
    let steps: Vec<Value> = operations
        .iter()
        .take(entry.expect.steps_total)
        .enumerate()
        .map(|(index, step)| {
            if let Some(failure) = entry
                .expect
                .failed_step
                .as_ref()
                .filter(|failure| failure.index == index)
            {
                json!({
                    "index": index,
                    "op": step["op"],
                    "status": "failed",
                    "error": failure.error_prefix,
                })
            } else {
                json!({
                    "index": index,
                    "op": step["op"],
                    "status": "passed",
                    "error": Value::Null,
                })
            }
        })
        .collect();
    let captures: Vec<Value> = entry
        .expect
        .capture_names
        .iter()
        .map(|name| json!({"name": name}))
        .collect();
    let report = json!({
        "schema": 1,
        "status": entry.expect.report_status,
        "steps": steps,
        "captures": captures,
    });
    fs::write(
        directory.join(driver_report_name(&entry.path)),
        serde_json::to_vec_pretty(&report).must("serialize synthetic report"),
    )
    .must("write synthetic report");
}

fn write_synthetic_shard(
    root: &Path,
    required: &[&ScenarioEntry],
    shard_index: usize,
    shard_count: usize,
) {
    let directory = root.join(format!("shard-{shard_index}"));
    fs::create_dir(&directory).must("driver test setup");
    let selected: Vec<&&ScenarioEntry> = required
        .iter()
        .skip(shard_index)
        .step_by(shard_count)
        .collect();
    for entry in &selected {
        write_synthetic_report(&directory, entry);
    }
    let manifest_bytes = fs::read(repo_path(MANIFEST_PATH)).must("driver test setup");
    let completion = json!({
        "schema": 1,
        "manifest_sha256": Sha256::digest(&manifest_bytes).to_string(),
        "platform": "linux",
        "selection": "required-shard",
        "shard_index": shard_index,
        "shard_count": shard_count,
        "required_count": required.len(),
        "executed_count": selected.len(),
        "scenarios": selected.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
    });
    fs::write(
        directory.join("_completion.json"),
        serde_json::to_vec_pretty(&completion).must("driver test setup"),
    )
    .must("driver test setup");
}

fn driver_report_name(path: &str) -> String {
    let mut relative = PathBuf::from(path);
    relative.set_extension("");
    format!(
        "{}.json",
        relative
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("__")
    )
}
