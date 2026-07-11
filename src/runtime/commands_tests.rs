//! Tests for the tmux command builder, kept in a sibling file so
//! `commands.rs` stays under the source-file-size hard limit.

use super::*;
use crate::domain::SandboxEngine;
use std::time::Duration;

fn base_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: String::new(),
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
    }
}

/// The local launch plan omits the `LLXPRT_DEBUG` env assignment when the
/// signature's `llxprt_debug` is empty, and that absence propagates through
/// [`local_pane_command_args`] so the llxprt child never sees a stale debug
/// flag.
///
/// Drives the real production path (`local_launch_plan` →
/// `local_pane_command_args`) rather than re-deriving the env vector inline,
/// so a regression in the env-building logic (a new condition, a renamed key)
/// is caught here (#173).
#[test]
fn llxprt_debug_env_is_omitted_when_empty() {
    let signature = base_signature();
    let plan = local_launch_plan(&signature);
    assert!(
        !plan.env.iter().any(|(key, _)| key == "LLXPRT_DEBUG"),
        "empty llxprt_debug must not produce an LLXPRT_DEBUG env entry: {:?}",
        plan.env
    );

    let args = local_pane_command_args(&plan);
    assert!(
        !args.iter().any(|arg| arg.starts_with("LLXPRT_DEBUG=")),
        "local pane command must not carry an LLXPRT_DEBUG= arg when debug is empty: {args:?}"
    );
}

#[test]
fn remote_tmux_command_wraps_run_as_user_once() {
    let remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "example.com".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: false,
    };

    let command = remote_tmux_command(&remote, "tmux has-session -t 'demo'");
    assert_eq!(
        command,
        "sudo -n su - 'acoliver' -c 'tmux has-session -t '\\''demo'\\'''"
    );
}

/// The remote-command timeout branch produces a `RuntimeError::RemoteExecutionFailed`
/// whose message names the context and the timeout. Drives the real
/// [`run_command_capture_with_timeout`] branch with a sub-second injectable
/// deadline against a portable `sleep` probe, so the test is fast and hermetic
/// (no `python3` dependency) (#173).
#[test]
fn remote_execution_timeout_returns_clear_error() {
    // `sleep` is specified by POSIX and present on every CI platform jefe
    // targets; a 2s sleep with a 1s deadline trips the timeout branch in well
    // under the 20s production default, so this test runs in ~1s.
    let mut cmd = Command::new("sleep");
    cmd.arg("2");

    let Err(RuntimeError::RemoteExecutionFailed(message)) =
        run_command_capture_with_timeout(cmd, Duration::from_secs(1), "timeout probe")
    else {
        panic!("expected remote timeout failure");
    };

    assert!(
        message.contains("timed out"),
        "message should name the timeout: {message}"
    );
    assert!(
        message.contains("timeout probe"),
        "message should name the context: {message}"
    );
    assert!(
        message.contains("after 1s"),
        "message should report the injected deadline as fractional seconds: {message}"
    );
}

/// A non-empty `llxprt_debug` signature produces an `LLXPRT_DEBUG=<value>`
/// env entry in the launch plan, and that assignment flows through
/// [`local_pane_command_args`] verbatim (single argv entry, no extra quoting),
/// so the llxprt child inherits the intended debug level.
///
/// Drives the real production path (`local_launch_plan` →
/// `local_pane_command_args`) instead of re-deriving the env vector inline
/// (#173).
#[test]
fn llxprt_debug_env_is_included_when_non_empty() {
    let mut signature = base_signature();
    signature.llxprt_debug = "trace=1".to_owned();

    let plan = local_launch_plan(&signature);
    assert_eq!(
        plan.env
            .iter()
            .find(|(key, _)| key == "LLXPRT_DEBUG")
            .map(|(_, value)| value.clone()),
        Some("trace=1".to_owned()),
        "non-empty llxprt_debug must produce an LLXPRT_DEBUG env entry: {:?}",
        plan.env
    );

    let args = local_pane_command_args(&plan);
    assert!(
        args.iter().any(|arg| arg == "LLXPRT_DEBUG=trace=1"),
        "local pane command must carry the LLXPRT_DEBUG=trace=1 argv entry verbatim: {args:?}"
    );
}

#[test]
fn remote_launch_command_enables_remain_on_exit() {
    let session_name = shell_escape_single("jefe-agent-test");
    let work_dir = shell_escape_single("/tmp/work");
    let cli_command = shell_escape_single("/tmp/work/node_modules/.bin/llxprt");
    let env_prefix = String::new();

    let tmux_script = build_remote_tmux_script(&work_dir, &env_prefix, &session_name, &cli_command);

    assert!(tmux_script.contains("tmux new-session -d -s 'jefe-agent-test'"));
    assert!(tmux_script.contains("set-option -t 'jefe-agent-test' remain-on-exit on"));
}

/// The `env -u` scrub prefix must strip every tmux client var so an agent's
/// bare `tmux` can never reach jefe's private server (#171).
#[test]
fn tmux_scrub_env_args_strips_all_tmux_client_vars() {
    let args = tmux_scrub_env_args();
    assert_eq!(
        args,
        vec![
            "env".to_owned(),
            "-u".to_owned(),
            "TMUX".to_owned(),
            "-u".to_owned(),
            "TMUX_PANE".to_owned(),
            "-u".to_owned(),
            "TMUX_TMPDIR".to_owned(),
        ]
    );
}

/// The local pane command must begin with the `env -u` scrub and place it
/// before `llxprt`, so the agent child never inherits jefe's tmux handle
/// (#171). Covers both the no-extra-env and with-env cases.
#[test]
fn local_pane_command_scrubs_tmux_env_before_llxprt() {
    let plan_no_env = LocalLaunchPlan {
        agent_kind: AgentKind::Llxprt,
        args: vec!["--continue".to_owned()],
        env: Vec::new(),
        warning: None,
    };
    let args = local_pane_command_args(&plan_no_env);
    assert_eq!(args[0], "env");
    assert_eq!(args[1], "-u");
    assert_eq!(args[2], "TMUX");
    assert_eq!(args[3], "-u");
    assert_eq!(args[4], "TMUX_PANE");
    assert_eq!(args[5], "-u");
    assert_eq!(args[6], "TMUX_TMPDIR");
    // scrub (7 entries) is immediately followed by llxprt + its args.
    assert_eq!(args[7], "llxprt", "scrub must immediately precede llxprt");
    assert_eq!(args[8], "--continue");

    // With an env assignment, the K=V must sit between the scrub and llxprt.
    let plan_with_env = LocalLaunchPlan {
        agent_kind: AgentKind::Llxprt,
        args: Vec::new(),
        env: vec![("LLXPRT_DEBUG".to_owned(), "trace=1".to_owned())],
        warning: None,
    };
    let args = local_pane_command_args(&plan_with_env);
    let scrub_end = args
        .windows(2)
        .rposition(|w| w[0] == "-u" && w[1] == "TMUX_TMPDIR");
    assert!(scrub_end.is_some(), "scrub must be present");
    let scrub_end = scrub_end.unwrap_or(0);
    assert_eq!(
        args[scrub_end + 2],
        "LLXPRT_DEBUG=trace=1",
        "env assignment must follow the scrub"
    );
    assert_eq!(
        args[scrub_end + 3],
        "llxprt",
        "llxprt must follow the env assignment"
    );
}

/// The remote launch script must prepend the `env -u` scrub to the pane
/// command so a remote agent cannot reach the (remote) tmux server hosting
/// it (#171). Exercises the real [`build_remote_tmux_script`] template plus
/// the real scrub/escape helpers rather than re-deriving the string.
#[test]
fn remote_launch_command_scrubs_tmux_env_from_pane() {
    let escaped_work_dir = shell_escape_single("/tmp/work");
    let escaped_session = shell_escape_single("jefe-agent-scrub");
    // Build the pane command from the same production helpers the launcher
    // uses, so a regression in any of them (removing the scrub, reordering
    // the template) is caught here.
    let cli_command = remote_cli_command(
        "/tmp/work/node_modules/.bin/llxprt",
        &launch_args(&base_signature()),
    );
    let env_scrub = tmux_scrub_env_args().join(" ");
    let pane_command = format!("{env_scrub} {cli_command}");

    let tmux_script = build_remote_tmux_script(
        &escaped_work_dir,
        "", // no sandbox/debug env exports in the base signature
        &escaped_session,
        &pane_command,
    );

    // The scrub prefix must appear before the llxprt command in the pane.
    let llxprt_pos = tmux_script.find("/tmp/work/node_modules/.bin/llxprt");
    let scrub_pos = tmux_script.find("env -u TMUX -u TMUX_PANE -u TMUX_TMPDIR");
    assert!(llxprt_pos.is_some(), "cli command should be present");
    assert!(scrub_pos.is_some(), "env scrub prefix should be present");
    assert!(
        scrub_pos < llxprt_pos,
        "env scrub must precede the llxprt command"
    );
}

/// `SANDBOX_FLAGS` must reach the local pane command as a single raw
/// `SANDBOX_FLAGS=--cpus=2 --memory=12288m --pids-limit=256` argv entry (no
/// shell quoting), because tmux passes each argv element to `sh -c` as one
/// token — shell-quoting the value would embed literal quote chars in the env
/// var the agent child sees.
///
/// Drives the real production path (`local_launch_plan` →
/// `local_pane_command_args`) with `sandbox_enabled: true` and asserts the raw
/// argv entry appears, rather than re-deriving `format!` output (#173).
#[test]
fn sandbox_flags_env_value_is_raw_for_tmux_argv() {
    let mut signature = base_signature();
    signature.sandbox_enabled = true;
    // Use the exact default flags the issue calls out so the assertion locks
    // the real value, not a hand-built literal.
    signature.sandbox_flags = "--cpus=2 --memory=12288m --pids-limit=256".to_owned();

    let plan = local_launch_plan(&signature);
    assert_eq!(
        plan.env
            .iter()
            .find(|(key, _)| key == "SANDBOX_FLAGS")
            .map(|(_, value)| value.clone()),
        Some("--cpus=2 --memory=12288m --pids-limit=256".to_owned()),
        "sandbox-enabled plan must carry the raw SANDBOX_FLAGS value: {:?}",
        plan.env
    );

    let args = local_pane_command_args(&plan);
    assert!(
        args.iter()
            .any(|arg| arg == "SANDBOX_FLAGS=--cpus=2 --memory=12288m --pids-limit=256"),
        "local pane command must carry the raw SANDBOX_FLAGS=... argv entry (no shell quoting): {args:?}"
    );
}

#[test]
fn tmux_base_args_include_config_skip_and_dedicated_socket() {
    let args = tmux_base_args();
    let socket = crate::runtime::jefe_tmux_socket_path();
    assert_eq!(
        args,
        vec![
            "-f".to_owned(),
            "/dev/null".to_owned(),
            "-S".to_owned(),
            socket.to_string_lossy().into_owned(),
        ]
    );
}

#[test]
fn parse_pane_pid_extracts_first_numeric_line() {
    assert_eq!(parse_pane_pid("12345\n"), Some(12_345));
    assert_eq!(parse_pane_pid("  98765  \n"), Some(98_765));
}

#[test]
fn parse_pane_pid_returns_none_for_empty_output() {
    assert_eq!(parse_pane_pid(""), None);
    assert_eq!(parse_pane_pid("   \n  \n"), None);
}

#[test]
fn parse_pane_pid_returns_none_for_garbage() {
    assert_eq!(parse_pane_pid("not-a-pid\n"), None);
    assert_eq!(parse_pane_pid("abc\ndef\n"), None);
}

#[test]
fn is_tmux_fork_broken_classifies_known_messages() {
    assert!(is_tmux_fork_broken("tmux: fork failed"));
    assert!(is_tmux_fork_broken(
        "open terminal failed: Device not configured"
    ));
    assert!(!is_tmux_fork_broken("session already exists"));
    assert!(!is_tmux_fork_broken(""));
}

#[test]
fn launch_args_emits_continue_when_pass_continue_true() {
    let signature = base_signature();
    assert!(
        launch_args(&signature)
            .iter()
            .any(|arg| arg == "--continue")
    );
}

#[test]
fn launch_args_omits_continue_when_pass_continue_false() {
    let mut signature = base_signature();
    signature.pass_continue = false;
    assert!(
        !launch_args(&signature)
            .iter()
            .any(|arg| arg == "--continue"),
        "issue-driven launches must never pass --continue"
    );
}

#[test]
fn code_puppy_launch_uses_only_supported_args() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.profile = "ignored-profile".to_owned();
    signature.mode_flags = vec!["--yolo".to_owned()];
    signature.pass_continue = true;
    signature.sandbox_enabled = true;

    assert_eq!(launch_args(&signature), vec!["-i"]);

    let plan = local_launch_plan(&signature);
    assert!(plan.env.is_empty());
    let pane_args = local_pane_command_args(&plan);
    assert!(pane_args.iter().any(|arg| arg == "code-puppy"));
    assert!(!pane_args.iter().any(|arg| arg == "llxprt"));
    assert!(!pane_args.iter().any(|arg| arg == "--continue"));
    assert!(!pane_args.iter().any(|arg| arg == "--sandbox"));
    assert!(!pane_args.iter().any(|arg| arg == "--profile-load"));
}

// ── Code Puppy strict args (issue #184) ───────────────────────────────────
//
// Code Puppy must output ONLY `-i` for normal launches, and `-i` plus the
// single positional instruction for fresh (issue/PR-driven) sends. Arbitrary
// mode_flags must never leak through.

#[test]
fn code_puppy_normal_launch_outputs_only_interactive_flag() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    // Normal launch with LLxprt-style flags that must be stripped.
    signature.mode_flags = vec!["--yolo".to_owned()];
    signature.pass_continue = true;

    assert_eq!(launch_args(&signature), vec!["-i"]);
}

#[test]
fn code_puppy_fresh_send_outputs_instruction_positional() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.mode_flags =
        vec!["Read and work on the GitHub issue described in .jefe/issue-prompt.md".to_owned()];
    signature.pass_continue = false;

    assert_eq!(
        launch_args(&signature),
        vec![
            "-i",
            "Read and work on the GitHub issue described in .jefe/issue-prompt.md"
        ]
    );
}

#[test]
fn code_puppy_strips_all_llxprt_only_flags() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.mode_flags = vec![
        "--yolo".to_owned(),
        "--profile-load".to_owned(),
        "ignored-profile".to_owned(),
        "--sandbox".to_owned(),
        "--continue".to_owned(),
        "Read and work on the GitHub PR described in .jefe/pr-prompt.md".to_owned(),
    ];

    assert_eq!(launch_args(&signature), vec!["-i"]);
}

#[test]
fn code_puppy_empty_mode_flags_outputs_only_interactive_flag() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.mode_flags = Vec::new();

    assert_eq!(launch_args(&signature), vec!["-i"]);
}

#[test]
fn code_puppy_discards_unrecognized_positional_flags() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.pass_continue = false;
    signature.mode_flags = vec![
        "first instruction".to_owned(),
        "second instruction".to_owned(),
    ];

    assert_eq!(launch_args(&signature), vec!["-i"]);
}
