//! Behavioral tests for the agent launcher command-spec seam (issue #269).
//!
//! These tests exercise the production `command_for_payload` function — the
//! same function used by `run_launch_plan` in production — to prove:
//!
//! - The program is `node.exe` (not `cmd.exe`).
//! - argv[0] is `npm-cli.js`.
//! - The adversarial package token (`--package=@vybestack/...<adversarial>`)
//!   is exactly ONE argv element.
//! - No `COMSPEC` / `cmd.exe` appears anywhere in the command.
//!
//! The seam is `agent_launcher::command_for_payload`, which takes a
//! deserialized `AgentLaunchPayload` and returns a `Command`. Tests construct
//! the payload directly (no I/O) and inspect the resulting `Command`'s program
//! and arguments via `std::process::Command::get_program()` and
//! `get_args()` (stable since Rust 1.57).

use std::ffi::OsString;
use std::path::PathBuf;

use super::agent_launcher::{
    AgentLaunchPayload, AgentWrapperKindPayload, NpmDirectInvocationPayload, command_for_payload,
};

/// Build a payload with an npm direct invocation carrying the adversarial
/// package token as a single argv element.
fn adversarial_npm_direct_payload(adversarial: &str) -> AgentLaunchPayload {
    let package_token = format!("--package=@vybestack/llxprt-code@{adversarial}");
    AgentLaunchPayload {
        path: PathBuf::from("/fake/npm.cmd"),
        wrapper: AgentWrapperKindPayload::CommandScript,
        npm_direct: Some(NpmDirectInvocationPayload {
            node_executable: PathBuf::from("/fake/node.exe"),
            cli_script: PathBuf::from("/fake/node_modules/npm/bin/npm-cli.js"),
        }),
        args: vec![
            "exec".into(),
            "--yes".into(),
            OsString::from(package_token),
            "--".into(),
            "llxprt".into(),
        ],
        environment: vec![],
    }
}

/// The production command-spec seam must use `node.exe` as the program (NOT
/// `cmd.exe`) when an npm direct invocation is present.
#[test]
fn command_spec_uses_node_exe_not_cmd_exe() {
    let payload = adversarial_npm_direct_payload("0.9.0");
    let command = command_for_payload(&payload);
    let program = command.get_program();
    assert!(
        program.to_string_lossy().ends_with("node.exe"),
        "program must be node.exe, got {program:?}"
    );
}

/// argv[0] must be `npm-cli.js` — the cli script, not the wrapper.
#[test]
fn command_spec_argv0_is_npm_cli_js() {
    let payload = adversarial_npm_direct_payload("0.9.0");
    let command = command_for_payload(&payload);
    let args: Vec<OsString> = command.get_args().map(OsString::from).collect();
    let argv0 = args
        .first()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    assert!(
        argv0.ends_with("npm-cli.js"),
        "argv[0] must be npm-cli.js, got argv: {args:?}"
    );
}

/// The adversarial package token must be exactly ONE argv element — never
/// split by cmd.exe metacharacters.
#[test]
fn command_spec_adversarial_package_token_is_one_argv_element() {
    let adversarial = "0.9.0 & calc.exe | whoami < nul > out ^ %VAR% !";
    let expected_token = format!("--package=@vybestack/llxprt-code@{adversarial}");
    let payload = adversarial_npm_direct_payload(adversarial);
    let command = command_for_payload(&payload);
    let args: Vec<String> = command
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();

    // The package token must appear exactly once.
    let count = args.iter().filter(|a| *a == &expected_token).count();
    assert_eq!(
        count, 1,
        "adversarial package token must be exactly one argv element, got args: {args:?}"
    );

    // No cmd.exe metacharacter may appear as a standalone argv token.
    for metachar in ["&", "|", "<", ">", "^", "%", "!"] {
        let standalone = args.iter().filter(|a| *a == metachar).count();
        assert_eq!(
            standalone, 0,
            "metacharacter '{metachar}' must not be a standalone argv token"
        );
    }
}

/// No `COMSPEC` / `cmd.exe` must appear anywhere in the program or args when
/// an npm direct invocation is present.
#[test]
fn command_spec_no_comspec_or_cmd_exe() {
    let payload = adversarial_npm_direct_payload("0.9.0");
    let command = command_for_payload(&payload);
    let program = command.get_program().to_string_lossy().to_string();
    let args: Vec<String> = command
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();

    assert!(
        !program.eq_ignore_ascii_case("cmd.exe") && !program.eq_ignore_ascii_case("COMSPEC"),
        "program must not be cmd.exe or COMSPEC, got {program:?}"
    );

    for arg in &args {
        assert!(
            !arg.eq_ignore_ascii_case("cmd.exe")
                && !arg.eq_ignore_ascii_case("/D")
                && !arg.eq_ignore_ascii_case("/S")
                && !arg.eq_ignore_ascii_case("/C"),
            "no cmd.exe flags must appear in args, got {arg:?} in {args:?}"
        );
    }
}

/// When NO npm direct invocation is present (Direct wrapper kind), the
/// command must use the payload's path directly — proving the seam correctly
/// falls back to the wrapper strategy when npm_direct is absent.
#[test]
fn command_spec_direct_wrapper_uses_payload_path() {
    let payload = AgentLaunchPayload {
        path: PathBuf::from("/fake/llxprt.exe"),
        wrapper: AgentWrapperKindPayload::Direct,
        npm_direct: None,
        args: vec!["--continue".into()],
        environment: vec![],
    };
    let command = command_for_payload(&payload);
    let program = command.get_program();
    assert!(
        program.to_string_lossy().ends_with("llxprt.exe"),
        "direct wrapper must use payload path, got {program:?}"
    );
    let args: Vec<String> = command
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();
    assert_eq!(args, vec!["--continue".to_owned()]);
}

/// A CommandScript wrapper WITHOUT npm_direct must use cmd.exe — proving the
/// fallback path is the ONLY path that uses cmd.exe, and the npm_direct
/// override is the safety mechanism.
#[cfg(windows)]
#[test]
fn command_spec_command_script_without_npm_direct_uses_cmd_exe() {
    let payload = AgentLaunchPayload {
        path: PathBuf::from("/fake/npm.cmd"),
        wrapper: AgentWrapperKindPayload::CommandScript,
        npm_direct: None,
        args: vec![],
        environment: vec![],
    };
    let command = command_for_payload(&payload);
    let program = command.get_program().to_string_lossy().to_string();
    // On Windows, the fallback uses COMSPEC or cmd.exe. This test proves the
    // fallback exists; the safety invariant is that npm_direct bypasses it.
    assert!(
        program.eq_ignore_ascii_case("cmd.exe") || program.contains("COMSPEC"),
        "command script without npm_direct must use cmd.exe/COMSPEC, got {program:?}"
    );
}
