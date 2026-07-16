//! Tests for the tmux command builder, kept in a sibling file so
//! `commands.rs` stays under the source-file-size hard limit.

use super::*;
use crate::domain::SandboxEngine;
use crate::runtime::pane_capture::{capture_pane_history_args, parse_pane_pid};
#[cfg(unix)]
use std::process::Stdio;
#[cfg(unix)]
use std::time::Duration;
#[test]
fn remote_attach_plan_uses_direct_ssh_and_excludes_local_psmux_namespace() {
    let remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        port: Some(2222),
        identity_file: std::path::PathBuf::from(r"C:\Keys Ω\agent key"),
        options: vec!["Compression=yes".to_owned()],
        ..crate::domain::RemoteRepositorySettings::default()
    };
    let plan = crate::ssh::SshPlan::with_executable(
        std::path::PathBuf::from(r"C:\Program Files\OpenSSH\ssh.exe"),
        &remote,
        &remote_tmux_command(&remote, "tmux attach-session -t 'remote-agent'"),
        crate::ssh::SshMode::Terminal,
    )
    .unwrap_or_else(|error| panic!("plan remote attach: {error}"));
    assert_eq!(
        plan.executable(),
        std::path::Path::new(r"C:\Program Files\OpenSSH\ssh.exe")
    );
    assert!(plan.args().contains(&std::ffi::OsString::from("-tt")));
    let remote_command = plan.args().last().map_or_else(
        || panic!("remote attach plan should contain a command"),
        |argument| argument.to_string_lossy(),
    );
    assert!(remote_command.contains("tmux attach-session"));
    assert!(!remote_command.contains("psmux"));
    assert!(!remote_command.contains("JEFE_PSMUX"));
}

fn base_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: Some(false),
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
        llxprt_version: None,
    }
}

#[test]

fn code_puppy_omits_yolo_argument_for_legacy_unconfigured_agent() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_yolo = None;

    assert_eq!(code_puppy_launch_args(&signature), vec!["-i"]);
}

#[test]

fn code_puppy_quick_resume_uses_exact_work_dir_and_preserves_argv_order() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.work_dir = std::path::PathBuf::from("/remote/work/puppy");
    signature.code_puppy_quick_resume = true;
    signature.code_puppy_model = "puppy-pro".to_owned();
    signature.code_puppy_yolo = Some(true);

    assert_eq!(
        code_puppy_launch_args(&signature),
        vec![
            "-i",
            "--quick-resume",
            "/remote/work/puppy",
            "--model",
            "puppy-pro",
            "--yolo",
            "true"
        ]
    );
}

#[test]

fn code_puppy_does_not_infer_quick_resume_from_llxprt_continue() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.pass_continue = true;

    let args = code_puppy_launch_args(&signature);
    assert!(!args.iter().any(|arg| arg == "--continue"));
    assert!(!args.iter().any(|arg| arg == "--quick-resume"));
}

#[test]

fn code_puppy_omits_model_argument_when_unset() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;

    assert_eq!(
        code_puppy_launch_args(&signature),
        vec!["-i", "--yolo", "false"]
    );
}

#[test]

fn code_puppy_passes_configured_model_as_exact_argv() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_model = "  openrouter/puppy-pro  ".to_owned();

    assert_eq!(
        code_puppy_launch_args(&signature),
        vec!["-i", "--model", "openrouter/puppy-pro", "--yolo", "false"]
    );
}

#[test]

fn llxprt_ignores_code_puppy_model() {
    let mut signature = base_signature();
    signature.code_puppy_model = "puppy-only".to_owned();

    let args = llxprt_launch_args(&signature);

    assert!(!args.iter().any(|arg| arg == "--model"));
    assert!(!args.iter().any(|arg| arg == "puppy-only"));
}

#[test]

fn code_puppy_passes_explicit_true_yolo_value() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_yolo = Some(true);

    assert_eq!(
        code_puppy_launch_args(&signature),
        vec!["-i", "--yolo", "true"]
    );
}

#[test]

fn code_puppy_fresh_prompt_keeps_model_before_instruction() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_model = "puppy-pro".to_owned();
    signature.pass_continue = false;
    signature.mode_flags = vec!["Read the issue prompt".to_owned()];

    assert_eq!(
        code_puppy_launch_args(&signature),
        vec![
            "-i",
            "--model",
            "puppy-pro",
            "--yolo",
            "false",
            "Read the issue prompt"
        ]
    );
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
        ..crate::domain::RemoteRepositorySettings::default()
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
#[cfg(unix)]
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
    command_capture_drains_stdout_and_stderr_beyond_pipe_capacity();
    command_capture_timeout_terminates_descendant_process_group();
}

#[cfg(unix)]
fn command_capture_drains_stdout_and_stderr_beyond_pipe_capacity() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "head -c 1048576 /dev/zero; head -c 1048576 /dev/zero >&2",
    ]);

    let output =
        run_command_capture_with_timeout(command, Duration::from_secs(5), "large output probe")
            .unwrap_or_else(|error| panic!("large output should not deadlock: {error}"));

    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 1_048_576);
    assert_eq!(output.stderr.len(), 1_048_576);
}

#[cfg(unix)]
fn command_capture_timeout_terminates_descendant_process_group() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
    let pid_file = directory.path().join("descendant.pid");
    let script = format!(
        "sleep 30 & printf '%s' $! > {}; wait",
        shell_escape_single(&pid_file.to_string_lossy())
    );
    let mut command = Command::new("sh");
    command.args(["-c", &script]);

    let result = run_command_capture_with_timeout(
        command,
        Duration::from_millis(100),
        "descendant cleanup probe",
    );
    assert!(matches!(
        result,
        Err(RuntimeError::RemoteExecutionFailed(message)) if message.contains("timed out")
    ));

    let pid = std::fs::read_to_string(&pid_file)
        .unwrap_or_else(|error| panic!("descendant pid should be captured: {error}"));
    let mut alive = true;
    for _ in 0..20 {
        alive = Command::new("kill")
            .args(["-0", pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !alive,
        "timed-out descendant process {pid} must be terminated"
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

/// The remote tmux startup script must disable both the primary and secondary
/// tmux prefix keys so the remote attach client does not consume `C-b`
/// (`0x02`) from application control chords like Code Puppy's
/// `Ctrl-X Ctrl-B` (`0x18 0x02`) before they reach the agent (#200). The
/// options must target the exact session and appear after `remain-on-exit`.
#[test]
fn remote_launch_script_disables_tmux_prefix() {
    let session_name = shell_escape_single("jefe-agent-prefix");
    let work_dir = shell_escape_single("/tmp/work");
    let cli_command = shell_escape_single("llxprt");
    let env_prefix = String::new();

    let tmux_script = build_remote_tmux_script(&work_dir, &env_prefix, &session_name, &cli_command);

    assert!(
        tmux_script.contains("set-option -t 'jefe-agent-prefix' prefix None"),
        "remote script must disable the primary tmux prefix: {tmux_script}"
    );
    assert!(
        tmux_script.contains("set-option -t 'jefe-agent-prefix' prefix2 None"),
        "remote script must disable the secondary tmux prefix: {tmux_script}"
    );

    // The prefix options must follow remain-on-exit so the session exists
    // before the options are applied (regression guard against reordering).
    let Some(remain_pos) = tmux_script.find("set-option -t 'jefe-agent-prefix' remain-on-exit on")
    else {
        panic!("remain-on-exit missing: {tmux_script}");
    };
    let Some(prefix_pos) = tmux_script.find("set-option -t 'jefe-agent-prefix' prefix None") else {
        panic!("prefix None missing: {tmux_script}");
    };
    assert!(
        prefix_pos > remain_pos,
        "prefix option must appear after remain-on-exit: {tmux_script}"
    );
}

/// The remote reattach command for disabling the prefix targets the exact
/// session (escaped), sets both `prefix` and `prefix2` in one tmux invocation
/// using the `\;` separator, and is wrapped through the remote user-escalation
/// path. This is the reattach-side remediation for pre-existing remote
/// sessions (#200).
#[test]
fn remote_disable_prefix_command_targets_session_with_both_options() {
    let remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "example.com".to_owned(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..crate::domain::RemoteRepositorySettings::default()
    };

    let command = remote_disable_prefix_command(&remote, "jefe-agent-prefix");

    // Wrapped through the remote user path: no run_as_user means the inner
    // command is used verbatim.
    assert!(
        command.contains("tmux set-option -t 'jefe-agent-prefix' prefix None"),
        "remote disable-prefix command must set prefix None: {command}"
    );
    assert!(
        command.contains("set-option -t 'jefe-agent-prefix' prefix2 None"),
        "remote disable-prefix command must set prefix2 None: {command}"
    );
    // Single tmux invocation with the command separator, not two tmux calls.
    assert!(
        command.contains(r"\;"),
        "remote disable-prefix command must use the tmux command separator: {command}"
    );
    assert_eq!(
        command.matches("tmux set-option").count(),
        1,
        "both options must be in one tmux invocation: {command}"
    );
}

/// A remote with a distinct `run_as_user` wraps the disable-prefix fragment
/// through `sudo -n su`, matching the rest of the remote command path.
#[test]
fn remote_disable_prefix_command_wraps_through_run_as_user() {
    let remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "example.com".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: false,
        ..crate::domain::RemoteRepositorySettings::default()
    };

    let command = remote_disable_prefix_command(&remote, "s");
    assert!(
        command.starts_with("sudo -n su - 'acoliver' -c "),
        "run_as_user must wrap the command: {command}"
    );
    assert!(
        command.contains("prefix None"),
        "wrapped command must still disable prefix: {command}"
    );
}

/// The shared `prefix_disable_tmux_subcommands` builder emits one
/// `set-option` sub-command per option from `prefix_options_for_passthrough`,
/// separated by tmux's `\;`, with no leading `tmux` keyword. Locking this
/// format guards both the remote reattach fragment and the remote creation
/// script against drift (#200).
#[test]
fn prefix_disable_tmux_subcommands_joins_all_options_with_separator() {
    let seq = prefix_disable_tmux_subcommands("'s'");
    assert_eq!(
        seq, r"set-option -t 's' prefix None \; set-option -t 's' prefix2 None",
        "sub-commands must cover every option joined by the tmux separator"
    );
    // No leading tmux keyword: callers embed this in their own tmux context.
    assert!(
        !seq.starts_with("tmux"),
        "sub-command sequence must not include the leading tmux keyword: {seq}"
    );
    // Every production option appears as its own `set-option -t 's' <name>`
    // sub-command exactly once (matched on the full " None" suffix so "prefix"
    // is not double-counted inside "prefix2").
    for option in prefix_options_for_passthrough() {
        let needle = format!(" {option} None");
        assert_eq!(
            seq.matches(&needle).count(),
            1,
            "option {option} must appear exactly once: {seq}"
        );
    }
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
        executable: super::super::agent_executable::AgentExecutableTarget::Agent(AgentKind::Llxprt),
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
        executable: super::super::agent_executable::AgentExecutableTarget::Agent(AgentKind::Llxprt),
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

fn local_multiplexer_plan_uses_platform_isolation() {
    let plan = MultiplexerPlan::current()
        .unwrap_or_else(|error| panic!("local multiplexer plan should resolve: {error}"));
    if cfg!(windows) {
        assert!(plan.base_args().iter().any(|arg| arg == "-L"));
        assert!(!plan.base_args().iter().any(|arg| arg == "/dev/null"));
        assert!(!plan.base_args().iter().any(|arg| arg == "-S"));
    } else {
        let socket = crate::runtime::jefe_tmux_socket_path();
        assert_eq!(
            plan.base_args(),
            [
                std::ffi::OsString::from("-f"),
                std::ffi::OsString::from("/dev/null"),
                std::ffi::OsString::from("-S"),
                socket.as_os_str().to_owned(),
            ]
        );
    }
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

    assert_eq!(launch_args(&signature), vec!["-i", "--yolo", "false"]);

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
// Code Puppy outputs interactive mode plus its typed YOLO value for normal
// launches, and appends one positional instruction for fresh sends. Arbitrary
// LLxprt mode_flags must never leak through.

#[test]

fn code_puppy_normal_launch_outputs_only_interactive_flag() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    // Normal launch with LLxprt-style flags that must be stripped.
    signature.mode_flags = vec!["--yolo".to_owned()];
    signature.pass_continue = true;

    assert_eq!(launch_args(&signature), vec!["-i", "--yolo", "false"]);
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
            "--yolo",
            "false",
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

    assert_eq!(launch_args(&signature), vec!["-i", "--yolo", "false"]);
}

#[test]

fn code_puppy_empty_mode_flags_outputs_only_interactive_flag() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.mode_flags = Vec::new();

    assert_eq!(launch_args(&signature), vec!["-i", "--yolo", "false"]);
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

    assert_eq!(launch_args(&signature), vec!["-i", "--yolo", "false"]);
}

// ── Issue #198: history-aware capture-pane argv builder ───────────────────
//
// `capture-pane -p -S -<N> -E -` captures the last N lines of tmux scrollback
// history including the visible pane. The argv builder is a pure function so
// it can be unit-tested without spawning tmux.

/// The history-capture argv builder produces the correct flag sequence for a
/// bounded history request.
#[test]
fn capture_pane_history_argv_builds_correct_flags() {
    let argv = capture_pane_history_args("jefe-agent-1", 500);
    // Must include capture-pane, -p (plain text), -t <session>, -S -<N>, -E -
    assert!(argv.contains(&"capture-pane".to_owned()), "argv: {argv:?}");
    assert!(argv.contains(&"-p".to_owned()), "argv: {argv:?}");
    assert!(argv.contains(&"-t".to_owned()), "argv: {argv:?}");
    assert!(argv.contains(&"jefe-agent-1".to_owned()), "argv: {argv:?}");
    assert!(argv.contains(&"-S".to_owned()), "missing -S flag: {argv:?}");
    assert!(argv.contains(&"-E".to_owned()), "missing -E flag: {argv:?}");
    // -S value must be the negation of the requested line count.
    let s_idx = argv.iter().position(|a| a == "-S");
    let Some(s_idx) = s_idx else {
        panic!("-S must be present: {argv:?}");
    };
    let Some(s_value) = argv.get(s_idx + 1) else {
        panic!("-S must be followed by a value: {argv:?}");
    };
    assert_eq!(*s_value, "-500", "-S value must be -<history_lines>");
    // -E value must be "-" (end at the bottom of the visible pane).
    let e_idx = argv.iter().position(|a| a == "-E");
    let Some(e_idx) = e_idx else {
        panic!("-E must be present: {argv:?}");
    };
    let Some(e_value) = argv.get(e_idx + 1) else {
        panic!("-E must be followed by a value: {argv:?}");
    };
    assert_eq!(*e_value, "-", "-E value must be -");
}

/// The history-capture argv builder respects the requested line count.
#[test]
fn capture_pane_history_argv_respects_line_count() {
    let argv = capture_pane_history_args("jefe-agent-1", 2000);
    let s_idx = argv.iter().position(|a| a == "-S");
    let Some(s_idx) = s_idx else {
        panic!("-S must be present: {argv:?}");
    };
    let Some(s_value) = argv.get(s_idx + 1) else {
        panic!("-S must have a value: {argv:?}");
    };
    assert_eq!(*s_value, "-2000");
}

/// Zero history lines is clamped to 1 so -S is always a negative offset,
/// never the ambiguous `-S 0` (which means "capture entire scrollback").
#[test]
fn capture_pane_history_argv_zero_lines_clamps_to_one() {
    let argv = capture_pane_history_args("jefe-agent-1", 0);
    let s_idx = argv.iter().position(|a| a == "-S");
    let Some(s_idx) = s_idx else {
        panic!("-S must be present: {argv:?}");
    };
    let Some(s_value) = argv.get(s_idx + 1) else {
        panic!("-S must have a value: {argv:?}");
    };
    assert_eq!(*s_value, "-1", "zero lines should clamp to -S -1");
}
