//! Real-process contract for provider-free binding explanation.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const JEFE: &str = env!("CARGO_BIN_EXE_jefe");

fn unique_dir(label: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jefe_explain_process_{label}_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create process fixture: {error}"));
    dir
}

#[test]
fn explain_runs_offline_before_provider_runtime_or_tui_initialization() {
    let dir = unique_dir("offline");
    let source =
        b"settings_schema = 2\n[keymap.dashboard]\n\"dashboard.navigate-down\" = [\"x\"]\n";
    std::fs::write(dir.join("settings.toml"), source)
        .unwrap_or_else(|error| panic!("seed settings: {error}"));
    let empty_path = dir.join("offline-path");
    std::fs::create_dir_all(&empty_path)
        .unwrap_or_else(|error| panic!("create empty path: {error}"));

    let output = Command::new(JEFE)
        .args([
            "explain",
            "binding",
            "x",
            "--context",
            "dashboard",
            "--config",
        ])
        .arg(&dir)
        .env("PATH", &empty_path)
        .env("GH_TOKEN", "")
        .output()
        .unwrap_or_else(|error| panic!("run jefe explain: {error}"));

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("winner: dashboard.navigate-down"));
    assert!(stdout.contains("provenance: settings:"));
    assert_eq!(
        std::fs::read(dir.join("settings.toml")).unwrap_or_default(),
        source
    );
    assert!(!dir.join("state.json").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_process_exit_codes_distinguish_invalid_unresolved_and_usage() {
    let dir = unique_dir("codes");
    let run = |args: &[&str]| {
        Command::new(JEFE)
            .args(args)
            .arg("--config")
            .arg(&dir)
            .output()
            .unwrap_or_else(|error| panic!("run explain exit fixture: {error}"))
    };
    let invalid = run(&["explain", "binding", "Ctrl+"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("KEY-E401"));
    let unresolved = run(&["explain", "binding", "F24", "--context", "dashboard"]);
    assert_eq!(unresolved.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unresolved.stdout).contains("resolution: unbound"));
    let usage = Command::new(JEFE)
        .args(["explain", "binding"])
        .output()
        .unwrap_or_else(|error| panic!("run explain usage fixture: {error}"));
    assert_eq!(usage.status.code(), Some(64));
    let _ = std::fs::remove_dir_all(dir);
}
