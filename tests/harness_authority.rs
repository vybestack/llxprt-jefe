//! Source-level ownership contracts for the schema-1 harness cutover (#397).

use std::path::{Path, PathBuf};

const FORBIDDEN_PATHS: [&str; 25] = [
    "scripts/issue189-run-pr-scenario.sh",
    "scripts/issue194-run-scenario.sh",
    "scripts/issue222-run-scenario.sh",
    "scripts/issue230-run-scenario.sh",
    "scripts/issue238-run-scenario.sh",
    "scripts/issue241-capture.sh",
    "scripts/issue265-run-scenario.sh",
    "scripts/issue269-run-scenario.sh",
    "scripts/issue351-run-scenario.sh",
    "scripts/issue364-manager-run-scenario.sh",
    "scripts/issue621-run-scenarios.sh",
    concat!("src/bin/jefe-tmux", "-harness.rs"),
    "src/harness/capture.rs",
    "src/harness/error.rs",
    "src/harness/psmux_driver.rs",
    "src/harness/psmux_driver_tests.rs",
    "src/harness/psmux_process.rs",
    "src/harness/signal_cleanup.rs",
    "src/harness/signal_cleanup_tests.rs",
    "src/harness/tmux_driver.rs",
    "src/harness/tmux_driver_tests.rs",
    concat!("src/harness/v1/tmux", "_runner.rs"),
    "tests/fixtures/first_agent_tutorial/fake-capture.sh",
    "tests/issue241_capture.rs",
    "tests/ui/dashboard_reorder_tui.rs",
];
const FORBIDDEN_TEXT: [&str; 7] = [
    concat!("jefe-tmux", "-harness"),
    concat!("run_tmux", "_v1"),
    concat!("tmux", "_runner"),
    concat!("Psmux", "Driver"),
    concat!("Tmux", "Driver"),
    concat!("--harness", "-bin"),
    concat!("--jefe", "-bin"),
];

#[test]
fn sole_schema_1_authority_has_no_legacy_binary_runner_or_predecessor() {
    let mut errors = Vec::new();
    for relative in FORBIDDEN_PATHS {
        if repo_path(relative).exists() {
            errors.push(format!("forbidden predecessor still exists: {relative}"));
        }
    }
    for path in tracked_source_and_contract_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        for forbidden in FORBIDDEN_TEXT {
            if text.contains(forbidden) {
                errors.push(format!(
                    "{} still invokes or names forbidden authority {forbidden}",
                    display_repo_path(&path)
                ));
            }
        }
    }

    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn surviving_public_authority_is_runner_run() {
    let v1 = read_repo_text("src/harness/v1/mod.rs");
    assert!(v1.contains("pub mod runner;"));
    assert!(v1.contains("pub use runner::{RunOutcome, RunnerConfig, run};"));
    assert!(!v1.contains(concat!("pub mod tmux", "_runner;")));

    let cli = read_repo_text("src/bin/tmux_scenario.rs");
    assert!(cli.contains("harness::v1::runner"));
    assert!(!cli.contains(concat!("tmux", "_runner")));
}

fn tracked_source_and_contract_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in ["src", "tests", "scripts", "dev-docs", "docs", ".github"] {
        collect_text_files(&repo_path(root), &mut files);
    }
    for relative in ["Cargo.toml", "CONTRIBUTING.md"] {
        files.push(repo_path(relative));
    }
    files
}

fn collect_text_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("read entry in {}: {err}", directory.display()))
            .path();
        if path.is_dir() {
            collect_text_files(&path, files);
        } else if path != repo_path("dev-docs/testing/scenario-owner-evidence.json")
            && path.extension().is_some_and(|extension| {
                matches!(
                    extension.to_str(),
                    Some("rs" | "py" | "json" | "md" | "sh" | "yml" | "yaml" | "toml")
                )
            })
        {
            files.push(path);
        }
    }
}

fn display_repo_path(path: &Path) -> String {
    path.strip_prefix(repo_path(""))
        .unwrap_or_else(|err| panic!("strip repository prefix: {err}"))
        .display()
        .to_string()
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
