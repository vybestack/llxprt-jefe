//! Tests for the tmux command builder, kept in a sibling file so
//! `commands.rs` stays under the source-file-size hard limit.

use super::*;
use crate::domain::SandboxEngine;

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
    }
}

#[test]
fn llxprt_debug_env_is_omitted_when_empty() {
    let signature = base_signature();
    let mut launch_env: Vec<(String, String)> = Vec::new();

    if signature.sandbox_enabled {
        launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
    }
    if !signature.llxprt_debug.is_empty() {
        launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
    }

    assert!(!launch_env.iter().any(|(key, _)| key == "LLXPRT_DEBUG"));
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

#[test]
fn remote_execution_timeout_returns_clear_error() {
    let mut cmd = Command::new("python3");
    cmd.args(["-c", "import time; time.sleep(30)"]);

    let Err(RuntimeError::RemoteExecutionFailed(message)) =
        run_command_capture(cmd, "timeout probe")
    else {
        panic!("expected remote timeout failure");
    };

    assert!(message.contains("timed out"));
    assert!(message.contains("timeout probe"));
}

#[test]
fn llxprt_debug_env_is_included_when_non_empty() {
    let mut signature = base_signature();
    signature.llxprt_debug = "trace=1".to_owned();

    let mut launch_env: Vec<(String, String)> = Vec::new();
    if signature.sandbox_enabled {
        launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
    }
    if !signature.llxprt_debug.is_empty() {
        launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
    }

    assert_eq!(
        launch_env
            .into_iter()
            .find(|(key, _)| key == "LLXPRT_DEBUG")
            .map(|(_, value)| value),
        Some("trace=1".to_owned())
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

#[test]
fn sandbox_flags_env_value_is_raw_for_tmux_argv() {
    let key = "SANDBOX_FLAGS";
    let value = "--cpus=2 --memory=12288m --pids-limit=256";
    let arg = format!("{key}={value}");
    // Rust's Command::arg() passes this as a single argv entry to tmux.
    // tmux escapes each argument when constructing its sh -c command, so
    // spaces survive without extra quoting.  Adding shell_escape_single()
    // would embed literal quote characters in the env var value.
    assert_eq!(
        arg,
        "SANDBOX_FLAGS=--cpus=2 --memory=12288m --pids-limit=256"
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
