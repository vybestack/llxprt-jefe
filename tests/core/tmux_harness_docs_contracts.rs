//! Contracts for tmux harness docs and shipped scenario examples.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P05
//! @requirement REQ-TMUX-HARNESS-005

use std::path::{Path, PathBuf};

use jefe::harness::v1::parse_scenario_v1;

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
        "psmux 3.3.7",
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
        "dev-docs/tmux-scenarios/startup-quit.json",
        "JEFE_REQUIRE_PSMUX: \"1\"",
        "PSMUX_VERSION: \"3.3.7\"",
        "timeout-minutes:",
        "target/psmux-smoke",
        "target/tmux-harness",
    ] {
        assert!(
            workflow.contains(required),
            "native Windows CI must include {required:?}"
        );
    }
    // Issue #465: the psmux smoke suite runs exactly once via the workspace
    // test (`cargo test --workspace --all-features --locked` includes the
    // psmux-smoke feature). The duplicate explicit invocation was removed to
    // avoid doubling psmux process churn on CI.
    assert!(
        !workflow.contains("cargo test --features psmux-smoke --test psmux_smoke -- --nocapture"),
        "native Windows CI must not duplicate the psmux smoke suite (issue #465)"
    );
    assert!(
        workflow.contains("psmux-v$env:PSMUX_VERSION-windows-x64.zip")
            && workflow.contains("releases/download/v$env:PSMUX_VERSION/$archiveName")
            && workflow
                .contains("60ff7b236f64184921cef3c1ff2611aa5a36fcc7ed8e2a58e968b8ded57f6028"),
        "native Windows CI must pin and checksum the qualified psmux release"
    );
    assert!(
        workflow.contains("Where-Object { $_ -eq \"tmux $env:PSMUX_VERSION\" }")
            && workflow.contains("if ($null -eq $versionLine)"),
        "native Windows CI must accept psmux metadata after the qualified tmux version line"
    );
}

/// @requirement REQ-TMUX-HARNESS-005
/// @pseudocode component-002 lines 1-6
/// Issue #383 S8 (no-shim amendment D10): every shipped scenario — including
/// the nested per-issue directories — is strict schema-1. There is exactly one
/// scenario parser, so a legacy document now fails to parse rather than
/// silently taking a second code path.
#[test]
fn every_shipped_tmux_scenario_is_strict_schema_1() {
    // `harness-limits.json` deliberately declares an over-limit terminal so
    // `limits_fixture_fails_validation_before_any_launch` can prove the bound
    // is enforced before launch. It is schema-1 and must still be rejected.
    const REJECTION_FIXTURES: &[&str] = &["harness-limits.json"];

    let paths = shipped_scenario_paths();
    assert!(
        paths.len() >= 70,
        "expected the full converted corpus, found {}",
        paths.len()
    );
    for path in paths {
        let json = read_repo_text(&path);
        assert!(
            json.contains("\"schema\": 1"),
            "{} must be strict schema-1 after the no-shim conversion",
            path.display()
        );
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let parsed = parse_scenario_v1(json.as_bytes());
        if REJECTION_FIXTURES.contains(&name) {
            assert!(
                parsed.is_err(),
                "{} is a rejection fixture and must fail validation",
                path.display()
            );
        } else {
            parsed
                .unwrap_or_else(|err| panic!("{} should parse as schema-1: {err}", path.display()));
        }
    }
}

/// The superseded parser, adapter, and scenario model are deleted, so no
/// compatibility path can be reintroduced by accident.
#[test]
fn the_superseded_scenario_parser_is_absent() {
    for removed in [
        "src/harness/parser.rs",
        "src/harness/scenario.rs",
        "src/harness/step.rs",
        "src/harness/expand.rs",
        "src/harness/macro_def.rs",
        "src/harness/config.rs",
    ] {
        assert!(
            !repo_path(removed).exists(),
            "{removed} must be deleted by the no-shim conversion"
        );
    }
}
/// Issue #574: no integration-test source under `tests/` may construct a bare
/// direct `tmux` process. The harness owns all tmux access through
/// `TmuxDriver`, which pins every call to the private `-L jefe-harness-<pid>`
/// socket and scrubs the inherited tmux client env (#171, #173). A bare
/// command in a test resolves against the socket named by an inherited
/// `$TMUX` — exactly the bug #574 fixed: the reorder scenario's hand-rolled
/// cleanup issued a session kill on the outer server while never reaching the
/// harness session it was written to remove. Integration tests must route
/// every tmux operation through the driver (or `run_tmux_v1`) so teardown and
/// isolation stay a single source of truth.
///
/// The needle is assembled with `concat!` so its contiguous form never appears
/// in this file's own source, keeping the scan self-clean.
///
/// Scope: like the sibling doc/workflow contracts in this file, this is a
/// pragmatic source-text scan, not a data-flow analysis. It catches a direct
/// literal `tmux` `Command` construction (the exact shape of the #574
/// regression) but not variable indirection such as binding `"tmux"` to a
/// variable and passing that to `Command::new`. New integration tests must keep
/// routing tmux access through the driver, which is the real invariant; this
/// scan is the tripwire that stops the direct form coming back, not a static
/// analyzer.
#[test]
fn no_test_suite_source_constructs_a_bare_tmux_command() {
    let tests_dir = repo_path("tests");
    let mut rust_files = Vec::new();
    collect_rust_sources(&tests_dir, &mut rust_files);
    assert!(
        !rust_files.is_empty(),
        "expected to find integration-test sources under tests/"
    );

    let needle = concat!("Command::", "new(\"tmux\")");
    let mut hits = Vec::new();
    for file in &rust_files {
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        if source.contains(needle) {
            hits.push(file.display().to_string());
        }
    }
    assert!(
        hits.is_empty(),
        "integration tests must route tmux access through TmuxDriver (issue #574); \
         found a bare direct tmux command in: {hits:?}"
    );
}

fn shipped_scenario_paths() -> Vec<PathBuf> {
    let dir = repo_path("dev-docs/tmux-scenarios");
    let mut paths = read_json_paths(&dir);
    assert!(!paths.is_empty(), "no shipped scenario JSON files found");
    paths.sort();
    paths
}

fn read_json_paths(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("failed to read scenario entry: {err}"))
            .path();
        if path.is_dir() {
            found.extend(read_json_paths(&path));
        } else if path.extension().is_some_and(|ext| ext == "json") {
            found.push(path);
        }
    }
    found
}

/// Recursively collect every `.rs` file under `dir` (issue #574 contract scan).
///
/// Read failures panic rather than skip the subtree: this is a contract test,
/// so silently swallowing a `read_dir` error would let a violating file pass
/// undetected — the opposite of the test's intent.
fn collect_rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("failed to read entry in {}: {err}", dir.display()))
            .path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn read_repo_text(relative_path: impl AsRef<Path>) -> String {
    let path = repo_path(relative_path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}
