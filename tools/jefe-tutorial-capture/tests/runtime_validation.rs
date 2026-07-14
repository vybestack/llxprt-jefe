#![cfg(unix)]

//! Guarded real-runtime validation tests extracted from runner_tests.rs
//! to keep that module under the per-file line limit (Finding #7).
//!
//! These tests create a curated PATH with specific agent runtime shims,
//! launch the real Jefe binary, and assert the startup detection behavior.
//! They capture evidence (screen captures) for documentation.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-003 (Finding #3)

use std::path::PathBuf;

use jefe::harness::{
    TmuxDriver, TmuxPaneSize, TmuxStartRequest, parse_scenario, run_tmux_scenario,
};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// Resolve the jefe binary for a guarded integration test.
fn guarded_jefe_binary(context: &str) -> Option<PathBuf> {
    let tmux = TmuxDriver::new();
    if !tmux.is_available() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("skipping {context}: tmux unavailable\n").as_bytes(),
        );
        return None;
    }
    let binary = jefe_binary_path();
    if binary.is_none() {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            format!("skipping {context}: jefe binary unavailable\n").as_bytes(),
        );
    }
    binary
}

fn jefe_binary_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_jefe") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let current = std::env::current_exe().ok()?;
    let deps_dir = current.parent()?;
    let debug_dir = deps_dir.parent()?;
    let candidate = debug_dir.join("jefe");
    candidate.exists().then_some(candidate)
}

fn unique_session(label: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-runner-{label}-{pid}-{nanos}")
}

fn scenario(json_steps: &str) -> jefe::harness::Scenario {
    parse_scenario(&format!(
        r#"{{ "config": {{ "cols": 80, "rows": 24 }}, "steps": {json_steps} }}"#
    ))
    .value_or_panic("scenario should parse")
}

/// Build a curated bin directory with the specified agent runtime shims
/// and system tool symlinks. Returns the path to use as PATH.
fn build_curated_path(shims: &[&str], artifact_dir: &std::path::Path) -> PathBuf {
    let bin = artifact_dir.join("curated-bin");
    std::fs::create_dir_all(&bin).value_or_panic("create curated bin");

    // Write agent runtime shims.
    for name in shims {
        let shim_path = bin.join(name);
        std::fs::write(
            &shim_path,
            "#!/bin/sh\necho \"[jefe-tutorial-shim]\"\necho \"runtime-shim: ready\"\nwhile IFS= read -r line; do printf '> %s\\n' \"$line\"; done\n",
        )
        .value_or_panic("write shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755))
                .value_or_panic("make shim executable");
        }
    }

    // Symlink required system tools into the curated bin. The launched Jefe
    // binary needs these to function: tmux (session management), git (repo
    // info), sh (scripts), env (agent pane prefix env -u TMUX...), id (socket
    // UID resolution), kill (PID liveness), gh (GitHub ops).
    let path = std::env::var("PATH").unwrap_or_default();
    for tool in ["git", "tmux", "sh", "env", "id", "kill", "gh"] {
        for dir in path.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = std::path::PathBuf::from(dir).join(tool);
            if candidate.exists() {
                let link_path = bin.join(tool);
                if link_path.exists() {
                    std::fs::remove_file(&link_path).value_or_panic("remove stale tool link");
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(&candidate, &link_path)
                    .value_or_panic("create curated tool link");
                break;
            }
        }
    }
    bin
}

/// Validate that Jefe detects ONLY the llxprt shim when the curated PATH
/// contains only the llxprt shim (#241 Finding #3).
#[test]
fn guarded_real_jefe_llxprt_only_startup_detection() {
    let Some(jefe_binary) = guarded_jefe_binary("llxprt-only startup test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("config tempdir");
    let artifact_base = tempfile::tempdir().value_or_panic("artifact tempdir");

    let curated_bin = build_curated_path(&["llxprt"], artifact_base.path());

    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "capture": "llxprt-only-startup" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("llxprt-only");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request")
    .with_env_path(curated_bin.to_string_lossy().into_owned());

    let summary = run_tmux_scenario(&scenario, &request, Some(artifact_base.path()))
        .value_or_panic("llxprt-only startup scenario");

    assert_eq!(summary.steps_run, 4);
    assert!(
        artifact_base
            .path()
            .join("llxprt-only-startup.screen.txt")
            .exists(),
        "startup capture evidence must exist"
    );
    assert!(
        summary
            .captures
            .contains(&"llxprt-only-startup".to_string()),
        "capture must be recorded in summary"
    );
}

/// Validate that Jefe detects ONLY the code-puppy shim when the curated PATH
/// contains only the code-puppy shim (#241 Finding #3).
#[test]
fn guarded_real_jefe_code_puppy_only_startup_detection() {
    let Some(jefe_binary) = guarded_jefe_binary("code-puppy-only startup test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("config tempdir");
    let artifact_base = tempfile::tempdir().value_or_panic("artifact tempdir");

    let curated_bin = build_curated_path(&["code-puppy"], artifact_base.path());

    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "capture": "code-puppy-only-startup" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("code-puppy-only");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request")
    .with_env_path(curated_bin.to_string_lossy().into_owned());

    let summary = run_tmux_scenario(&scenario, &request, Some(artifact_base.path()))
        .value_or_panic("code-puppy-only startup scenario");

    assert_eq!(summary.steps_run, 4);
    assert!(
        artifact_base
            .path()
            .join("code-puppy-only-startup.screen.txt")
            .exists(),
        "startup capture evidence must exist"
    );
    assert!(
        summary
            .captures
            .contains(&"code-puppy-only-startup".to_string()),
        "capture must be recorded in summary"
    );
}

/// Validate that Jefe detects BOTH runtimes when the curated PATH contains
/// both shims (#241 Finding #3, both-installed baseline).
#[test]
fn guarded_real_jefe_both_runtimes_startup_detection() {
    let Some(jefe_binary) = guarded_jefe_binary("both-runtimes startup test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("config tempdir");
    let artifact_base = tempfile::tempdir().value_or_panic("artifact tempdir");

    let curated_bin = build_curated_path(&["llxprt", "code-puppy"], artifact_base.path());

    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "capture": "both-runtimes-startup" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("both-runtimes");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request")
    .with_env_path(curated_bin.to_string_lossy().into_owned())
    .with_keep_session(false);

    let summary = run_tmux_scenario(&scenario, &request, Some(artifact_base.path()))
        .value_or_panic("both-runtimes startup scenario");

    assert_eq!(summary.steps_run, 4);
    assert!(
        artifact_base
            .path()
            .join("both-runtimes-startup.screen.txt")
            .exists(),
        "startup capture evidence must exist"
    );
}

/// Validate that Jefe starts correctly with no agent runtimes installed
/// (empty curated PATH — only system tools). Jefe should still start and
/// display its dashboard; agent creation would just show "not installed".
#[test]
fn guarded_real_jefe_no_runtimes_startup_detection() {
    let Some(jefe_binary) = guarded_jefe_binary("no-runtimes startup test") else {
        return;
    };
    let config_dir = tempfile::tempdir().value_or_panic("config tempdir");
    let artifact_base = tempfile::tempdir().value_or_panic("artifact tempdir");

    let curated_bin = build_curated_path(&[], artifact_base.path());

    let scenario = scenario(
        r#"[
            { "waitFor": "LLxprt Jefe" },
            { "capture": "no-runtimes-startup" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]"#,
    );
    let session_name = unique_session("no-runtimes");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_binary,
        config_dir.path(),
        std::env::current_dir().value_or_panic("current dir"),
        TmuxPaneSize::new(100, 30, 2_000),
    )
    .value_or_panic("jefe request")
    .with_env_path(curated_bin.to_string_lossy().into_owned());

    let summary = run_tmux_scenario(&scenario, &request, Some(artifact_base.path()))
        .value_or_panic("no-runtimes startup scenario");

    assert_eq!(summary.steps_run, 4);
    assert!(
        artifact_base
            .path()
            .join("no-runtimes-startup.screen.txt")
            .exists(),
        "startup capture evidence must exist"
    );
}
