//! Contracts for tmux harness docs and shipped scenario examples.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P05
//! @requirement REQ-TMUX-HARNESS-005

use std::path::{Path, PathBuf};

use jefe::harness::{expand_macros, parse_scenario};

/// @plan PLAN-20260629-TMUX-HARNESS.P05
/// @requirement REQ-TMUX-HARNESS-005
/// @pseudocode component-001 lines 1-4
#[test]
fn dev_docs_index_links_to_tmux_harness_guide() {
    let readme = read_repo_text("dev-docs/README.md");

    assert!(
        readme.contains("[`tmux-harness.md`](./testing/tmux-harness.md)"),
        "dev-docs index should link the tmux harness guide (moved under testing/)"
    );
    assert!(
        readme.contains("[`psmux-smoke.md`](./testing/psmux-smoke.md)"),
        "dev-docs index should link the native Windows psmux smoke guide"
    );
}

/// @plan PLAN-20260629-TMUX-HARNESS.P05
#[test]
fn tmux_harness_guide_documents_native_windows_psmux_contract() {
    let guide = read_repo_text("dev-docs/testing/tmux-harness.md");
    for required in [
        "Native Windows with psmux",
        "psmux 3.3.6",
        "JEFE_PSMUX_BIN",
        "unique `psmux -L <namespace>`",
        "multiplexer.txt",
        "never invokes bare `psmux kill-server`",
        "WSL, Cygwin, MSYS2, Git Bash, Docker",
    ] {
        assert!(guide.contains(required), "guide must document {required:?}");
    }
}

#[test]
fn native_windows_ci_gates_psmux_and_startup_scenario() {
    let workflow = read_repo_text(".github/workflows/ci.yml");
    for required in [
        "runs-on: windows-latest",
        "target: x86_64-pc-windows-msvc",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo build --workspace --all-features --locked",
        "cargo test --workspace --all-features --locked",
        "cargo test --features psmux-smoke --test psmux_smoke -- --nocapture",
        "dev-docs/tmux-scenarios/startup-quit.json",
        "JEFE_REQUIRE_PSMUX: \"1\"",
        "timeout-minutes:",
        "target/psmux-smoke",
        "target/tmux-harness",
    ] {
        assert!(
            workflow.contains(required),
            "native Windows CI must include {required:?}"
        );
    }
    assert!(
        workflow.contains("psmux-v3.3.6-windows-x64.zip")
            && workflow
                .contains("a56a890ea0829567818b9a368f16dcbd39c087f27328573df17c10dd39618947"),
        "native Windows CI must pin and checksum the qualified psmux release"
    );
}

/// @requirement REQ-TMUX-HARNESS-005
/// @pseudocode component-002 lines 1-6
#[test]
fn shipped_tmux_scenarios_parse_and_expand() {
    for path in shipped_scenario_paths() {
        let json = read_repo_text(&path);
        let scenario = parse_scenario(&json)
            .unwrap_or_else(|err| panic!("{} should parse: {err}", path.display()));
        expand_macros(&scenario)
            .unwrap_or_else(|err| panic!("{} should expand: {err}", path.display()));
    }
}

fn shipped_scenario_paths() -> Vec<PathBuf> {
    let dir = repo_path("dev-docs/tmux-scenarios");
    let mut paths = read_json_paths(&dir);
    assert!(!paths.is_empty(), "no shipped scenario JSON files found");
    paths.sort();
    paths
}

fn read_json_paths(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("failed to read scenario entry: {err}"))
                .path()
        })
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect()
}

fn read_repo_text(relative_path: impl AsRef<Path>) -> String {
    let path = repo_path(relative_path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}
