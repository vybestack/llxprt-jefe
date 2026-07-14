//! Behavioral contracts for the Windows agent launcher's npm.cmd safety.
//!
//! When the resolved npm is `.cmd`/`.bat` on Windows, the launch plan must
//! carry a direct `node.exe` + `npm-cli.js` invocation so the selector and
//! all npm arguments remain structural argv that `cmd.exe` cannot reparse.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::agent_executable::{AgentWrapperKind, NpmDirectInvocation, ResolvedAgentExecutable};

/// Create a standard npm layout in a temp directory.
fn create_npm_layout(directory: &TempDir) -> PathBuf {
    let npm_cmd = directory.path().join("npm.cmd");
    std::fs::write(&npm_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npm.cmd: {e}"));
    std::fs::write(directory.path().join("node.exe"), b"binary")
        .unwrap_or_else(|e| panic!("write node.exe: {e}"));
    let bin = directory
        .path()
        .join("node_modules")
        .join("npm")
        .join("bin");
    std::fs::create_dir_all(&bin).unwrap_or_else(|e| panic!("mkdir bin: {e}"));
    std::fs::write(bin.join("npm-cli.js"), b"// cli").unwrap_or_else(|e| panic!("write cli: {e}"));
    npm_cmd
}

/// Build a `ResolvedAgentExecutable` carrying the derived `npm_direct`
/// invocation from a standard npm.cmd layout, cross-platform.
fn resolved_npm_cmd(directory: &TempDir) -> ResolvedAgentExecutable {
    let npm_cmd = create_npm_layout(directory);
    let invocation = NpmDirectInvocation::from_wrapper(&npm_cmd)
        .unwrap_or_else(|e| panic!("derive npm_direct: {e}"));
    ResolvedAgentExecutable::with_npm_direct_for_test(&npm_cmd, invocation)
}

/// Verify that a resolved npm.cmd carries the direct node invocation.
#[test]
fn resolved_npm_cmd_carries_npm_direct_invocation() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let resolved = resolved_npm_cmd(&directory);

    // The wrapper_kind stays CommandScript (so non-Windows or fallback paths
    // still work), but npm_direct must be populated.
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::CommandScript);
    let npm_direct = resolved.npm_direct().unwrap_or_else(|| {
        panic!("standard npm layout must derive npm_direct, resolved: {resolved:?}")
    });
    assert_eq!(
        npm_direct.node_executable(),
        directory.path().join("node.exe")
    );
    assert!(npm_direct.cli_script().ends_with("npm-cli.js"));
}

/// The launch plan payload must include `npm_direct` when present, so
/// `run_launch_plan` launches `node.exe` directly (never `cmd.exe`).
#[test]
fn launch_plan_serializes_npm_direct_for_round_trip() {
    use super::agent_launcher::write_launch_plan;

    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let resolved = resolved_npm_cmd(&directory);

    let args: Vec<OsString> = vec![
        "exec".into(),
        "--yes".into(),
        "--package=@vybestack/llxprt-code@0.9.0".into(),
        "--".into(),
        "llxprt".into(),
    ];
    let env: Vec<(OsString, OsString)> = vec![];

    let plan_path = write_launch_plan(&resolved, &args, &env)
        .unwrap_or_else(|e| panic!("write_launch_plan: {e}"));

    let bytes = std::fs::read(&plan_path).unwrap_or_else(|e| panic!("read plan: {e}"));
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse plan json: {e}"));

    // The payload must carry npm_direct with node_executable and cli_script.
    let npm_direct = json.get("npm_direct").unwrap_or_else(|| {
        panic!("payload must contain npm_direct field, json keys present");
    });
    assert!(
        npm_direct
            .get("node_executable")
            .is_some_and(|v| v.as_str().is_some_and(|s| s.ends_with("node.exe"))),
        "npm_direct.node_executable must end with node.exe"
    );
    assert!(
        npm_direct
            .get("cli_script")
            .is_some_and(|v| v.as_str().is_some_and(|s| s.ends_with("npm-cli.js"))),
        "npm_direct.cli_script must end with npm-cli.js"
    );
}

/// Adversarial selectors must survive as a single structural argv token in the
/// launch plan — never split by cmd.exe metacharacters.
#[test]
fn launch_plan_preserves_adversarial_selector_as_one_argv_token() {
    use super::agent_launcher::write_launch_plan;

    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let resolved = resolved_npm_cmd(&directory);

    let adversarial = "0.9.0 & calc.exe | whoami < nul > out ^ %VAR% !";
    let package_token = format!("--package=@vybestack/llxprt-code@{adversarial}");
    let args: Vec<OsString> = vec![
        "exec".into(),
        "--yes".into(),
        OsString::from(package_token.clone()),
        "--".into(),
        "llxprt".into(),
    ];

    let plan_path =
        write_launch_plan(&resolved, &args, &[]).unwrap_or_else(|e| panic!("plan: {e}"));
    let bytes = std::fs::read(&plan_path).unwrap_or_else(|e| panic!("read: {e}"));
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse: {e}"));

    // On Unix, serde serializes OsString as {"Unix": [byte array]}. On
    // Windows it serializes as a string. Decode each arg to verify the
    // adversarial selector survived as exactly one argv element.
    let args_array = json
        .get("args")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("payload must have args array, got: {json}"));

    let decode_arg = |v: &serde_json::Value| -> Option<String> {
        if let Some(s) = v.as_str() {
            return Some(s.to_owned());
        }
        // Unix: {"Unix": [bytes]}
        let byte_array = v.get("Unix").and_then(|u| u.as_array())?;
        let decoded = byte_array
            .iter()
            .filter_map(|b| b.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect::<Vec<u8>>();
        String::from_utf8(decoded).ok()
    };

    let decoded_args: Vec<String> = args_array
        .iter()
        .map(decode_arg)
        .map(Option::unwrap_or_default)
        .collect();

    // The adversarial package token must be exactly one element.
    let count = decoded_args.iter().filter(|a| *a == &package_token).count();
    assert_eq!(
        count, 1,
        "adversarial package token must be exactly one argv element, got args: {decoded_args:?}"
    );

    // cmd.exe metacharacters must NOT appear as standalone argv tokens.
    for metachar in ["&", "|", "<", ">", "^", "%", "!"] {
        let standalone_count = decoded_args.iter().filter(|a| *a == metachar).count();
        assert_eq!(
            standalone_count, 0,
            "metacharacter '{metachar}' must not be a standalone argv token"
        );
    }
}

/// When the resolved executable has no npm_direct (e.g. a Direct .exe), the
/// payload must have npm_direct as null/absent so `command_for_payload` uses
/// the wrapper strategy.
#[test]
fn launch_plan_has_null_npm_direct_for_direct_executable() {
    use super::agent_launcher::write_launch_plan;

    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_exe = directory.path().join("npm.exe");
    std::fs::write(&npm_exe, b"binary").unwrap_or_else(|e| panic!("write exe: {e}"));

    let resolved = ResolvedAgentExecutable::from_path(&npm_exe)
        .unwrap_or_else(|e| panic!("from_path for .exe should succeed: {e}"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::Direct);
    assert!(resolved.npm_direct().is_none());

    let plan_path = write_launch_plan(&resolved, &[], &[]).unwrap_or_else(|e| panic!("plan: {e}"));
    let bytes = std::fs::read(&plan_path).unwrap_or_else(|e| panic!("read: {e}"));
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse: {e}"));

    // npm_direct should be null (serde default serializes None as null).
    let npm_direct = json.get("npm_direct");
    assert!(
        npm_direct.is_some_and(serde_json::Value::is_null),
        "direct executable must have null npm_direct, got: {npm_direct:?}"
    );
}

/// Verify that `NpmDirectInvocation::from_wrapper` path derivation is correct
/// for a standard npm installation: node.exe is a sibling of npm.cmd, and
/// npm-cli.js is in `node_modules/npm/bin/` relative to the same directory.
#[test]
fn npm_direct_derives_paths_from_standard_layout() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = create_npm_layout(&directory);

    let invocation =
        NpmDirectInvocation::from_wrapper(&npm_cmd).unwrap_or_else(|e| panic!("derive: {e}"));

    let expected_node = directory.path().join("node.exe");
    let expected_cli = directory
        .path()
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");

    assert_eq!(invocation.node_executable(), expected_node);
    assert_eq!(invocation.cli_script(), expected_cli);
}

/// Verify path derivation does not depend on the wrapper path being absolute —
/// it works with relative paths too (important for test-determinism).
#[test]
fn npm_direct_derives_paths_from_relative_wrapper() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    create_npm_layout(&directory);

    // Change to the temp dir and use a relative path.
    let saved_cwd = std::env::current_dir().unwrap_or_else(|e| panic!("cwd: {e}"));
    std::env::set_current_dir(directory.path()).unwrap_or_else(|e| panic!("chdir: {e}"));

    let relative = Path::new("npm.cmd");
    let result = NpmDirectInvocation::from_wrapper(relative);

    // Restore cwd regardless of outcome.
    let _ = std::env::set_current_dir(&saved_cwd);

    let invocation = result.unwrap_or_else(|e| panic!("relative derive: {e}"));
    assert!(invocation.node_executable().ends_with("node.exe"));
}
