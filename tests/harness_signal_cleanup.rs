#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

trait TestResult<T> {
    fn must(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> TestResult<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|err| panic!("{context}: {err}"))
    }
}

#[test]
fn sigterm_reports_the_active_step_and_reaps_only_the_owned_group() {
    let fixture = TempFixture::new();
    let scenario = fixture.root.join("interrupt.json");
    write_scenario(&scenario);

    let mut sentinel = Sentinel::spawn();
    let runner = spawn_runner(&scenario);
    let runner_pid = runner.id();
    let probe_pid = wait_for_probe_child(runner_pid);
    std::thread::sleep(Duration::from_millis(100));
    assert!(sentinel.is_running());
    let signal_status = Command::new("/bin/kill")
        .args(["-TERM", &runner_pid.to_string()])
        .status()
        .must("signal runner");
    assert!(signal_status.success());

    let output = runner
        .wait_with_output()
        .must("wait for interrupted runner");
    assert_eq!(
        output.status.code(),
        Some(4),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse report: {err}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(report["status"], "failed");
    assert_eq!(report["steps"][1]["index"], 1);
    assert_eq!(report["steps"][1]["op"], "wait");
    assert_eq!(report["steps"][1]["status"], "failed");
    assert!(
        report["steps"][1]["error"]
            .as_str()
            .is_some_and(|error| error.contains("interrupted by SIGTERM"))
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("interrupted by SIGTERM"));
    assert_process_absent(probe_pid);
    assert!(sentinel.is_running());
}

fn write_scenario(scenario: &Path) {
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let scenario_body = r#"{
  "schema": 1,
  "name": "signal-interruption",
  "platform": "__PLATFORM__",
  "terminal": { "cols": 80, "rows": 24 },
  "workspace": {
    "mode": 448,
    "dirs": [{ "path": "work", "mode": 448 }],
    "files": [],
    "env": []
  },
  "steps": [
    { "op": "launch", "argv": ["probe"], "env": [], "cwd": "work" },
    { "op": "wait", "source": "frame", "literal": "NEVER ARRIVES", "timeout_ms": 30000 }
  ],
  "secrets": []
}
"#
    .replace("__PLATFORM__", platform);
    std::fs::write(scenario, scenario_body)
        .unwrap_or_else(|err| panic!("write {}: {err}", scenario.display()));
}

fn spawn_runner(scenario: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_tmux_scenario"))
        .arg("--scenario")
        .arg(scenario)
        .args([
            "--shim-bin",
            env!("CARGO_BIN_EXE_jefe-capture-shim"),
            "--install",
            &format!("probe={}", env!("CARGO_BIN_EXE_jefe-harness-probe")),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .must("spawn tmux_scenario")
}

fn wait_for_probe_child(parent: u32) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = Command::new("ps")
            .args(["-axo", "pid=,ppid=,command="])
            .output()
            .must("list processes");
        let processes = String::from_utf8_lossy(&output.stdout);
        for line in processes.lines() {
            let mut fields = line.split_whitespace();
            let Some(process_id) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            let Some(parent_id) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            if parent_id == parent && fields.any(|field| field.ends_with("/bin/probe")) {
                return process_id;
            }
        }
        assert!(
            Instant::now() < deadline,
            "probe did not start beneath runner {parent}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn assert_process_absent(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .must("probe process existence");
    assert!(
        !status.success(),
        "owned probe process {pid} survived cleanup"
    );
}

struct Sentinel(Child);

impl Sentinel {
    fn spawn() -> Self {
        Self(
            Command::new("/bin/sleep")
                .arg("30")
                .spawn()
                .unwrap_or_else(|err| panic!("spawn sentinel: {err}")),
        )
    }

    fn is_running(&mut self) -> bool {
        self.0.try_wait().must("query sentinel").is_none()
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        if self.0.try_wait().must("query sentinel").is_none() {
            self.0.kill().must("stop sentinel");
        }
        self.0.wait().must("reap sentinel");
    }
}

struct TempFixture {
    root: PathBuf,
}

impl TempFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "jefe-harness-signal-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .must("system time")
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap_or_else(|err| panic!("create {}: {err}", root.display()));
        Self { root }
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root)
            .unwrap_or_else(|err| panic!("remove {}: {err}", self.root.display()));
    }
}
