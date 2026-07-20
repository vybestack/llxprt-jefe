//! Structural and decision tests for shell-window lifecycle commands
//! (issue #361 PR A).
//!
//! These tests exercise the command-construction layer and pure parsing /
//! decision seams (not live tmux) so the Unix and Windows/psmux command
//! shapes and observation decisions stay correct without a multiplexer
//! dependency. Live behavior is proven by the tmux scenario in
//! `dev-docs/tmux-scenarios/agent-shell-overlay.json`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::{
    SHELL_WINDOW_NAME, close_all_shell_windows_plan, list_all_shell_windows_plan,
    list_shell_windows_plan, new_window_command, parse_sessions_with_shell_windows,
    parse_shell_window_names, select_window_command,
};
use crate::runtime::multiplexer::{LocalPlatform, MultiplexerIsolation, MultiplexerPlan};

fn plan(platform: LocalPlatform) -> MultiplexerPlan {
    let executable = if platform == LocalPlatform::Windows {
        PathBuf::from("psmux.exe")
    } else {
        PathBuf::from("tmux")
    };
    let isolation = if platform == LocalPlatform::Windows {
        MultiplexerIsolation::Namespace("jefe-test-lifecycle".to_owned())
    } else {
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe-shell-lifecycle.sock"))
    };
    MultiplexerPlan::for_platform(platform, executable, isolation)
        .unwrap_or_else(|error| panic!("test plan: {error}"))
}

// ---------------------------------------------------------------------------
// select-window (hide) command shape: Unix + psmux
// ---------------------------------------------------------------------------

#[test]
fn select_window_command_targets_window_zero_for_unix() {
    let command = select_window_command(&plan(LocalPlatform::Unix), "jefe-agent:0");
    let args = command.get_args().collect::<Vec<_>>();
    assert!(args.contains(&OsStr::new("select-window")));
    assert!(
        args.windows(2)
            .any(|pair| pair == [OsStr::new("-t"), OsStr::new("jefe-agent:0")]),
        "select-window must target jefe-agent:0, got args: {args:?}"
    );
}

#[test]
fn select_window_command_carries_psmux_namespace_prefix() {
    let command = select_window_command(&plan(LocalPlatform::Windows), "jefe-agent:0");
    let args = command.get_args().collect::<Vec<_>>();
    assert!(args.contains(&OsStr::new("select-window")));
    assert!(
        args.windows(2)
            .any(|pair| pair == [OsStr::new("-t"), OsStr::new("jefe-agent:0")])
    );
    // psmux `-L` namespace prefix invariant must be present on Windows.
    assert!(
        args.contains(&OsStr::new("-L")),
        "psmux select-window must carry -L namespace, got args: {args:?}"
    );
}

// ---------------------------------------------------------------------------
// Batched observation command shape: list-windows -a (the single supported
// path) across Unix + psmux.
// ---------------------------------------------------------------------------

#[test]
fn list_all_shell_windows_plan_uses_batched_all_sessions_for_unix() {
    let command = list_all_shell_windows_plan(&plan(LocalPlatform::Unix));
    let args = command.get_args().collect::<Vec<_>>();
    assert!(args.contains(&OsStr::new("list-windows")));
    assert!(
        args.contains(&OsStr::new("-a")),
        "batched observe must enumerate all sessions with -a, got args: {args:?}"
    );
    // The format string must join session and window name so orphan sessions
    // are discoverable.
    assert!(
        args.iter()
            .any(|arg| *arg == OsStr::new("#{session_name}:#{window_name}")),
        "batched observe must use session:window format, got args: {args:?}"
    );
}

#[test]
fn list_all_shell_windows_plan_carries_psmux_namespace() {
    let command = list_all_shell_windows_plan(&plan(LocalPlatform::Windows));
    let args = command.get_args().collect::<Vec<_>>();
    assert!(args.contains(&OsStr::new("list-windows")));
    assert!(args.contains(&OsStr::new("-a")));
    assert!(
        args.contains(&OsStr::new("-L")),
        "psmux batched observe must carry -L namespace, got args: {args:?}"
    );
}

// ---------------------------------------------------------------------------
// Bounded fallback command shape: per-session list-windows -t
// ---------------------------------------------------------------------------

#[test]
fn list_shell_windows_plan_targets_single_session_for_unix() {
    let command = list_shell_windows_plan(&plan(LocalPlatform::Unix), "jefe-agent");
    let args = command.get_args().collect::<Vec<_>>();
    assert!(args.contains(&OsStr::new("list-windows")));
    assert!(
        args.windows(2)
            .any(|pair| pair == [OsStr::new("-t"), OsStr::new("jefe-agent")]),
        "fallback list-windows must target the session, got args: {args:?}"
    );
    assert!(
        args.iter().any(|arg| *arg == OsStr::new("#{window_name}")),
        "fallback must request window names, got args: {args:?}"
    );
}

#[test]
fn list_shell_windows_plan_carries_psmux_namespace() {
    let command = list_shell_windows_plan(&plan(LocalPlatform::Windows), "jefe-agent");
    let args = command.get_args().collect::<Vec<_>>();
    assert!(
        args.contains(&OsStr::new("-L")),
        "psmux fallback observe must carry -L namespace, got args: {args:?}"
    );
}

// ---------------------------------------------------------------------------
// Pure parsing: parse_sessions_with_shell_windows (batched format)
// ---------------------------------------------------------------------------

#[test]
fn parse_sessions_extracts_jefe_shell_owners_deterministically() {
    let raw = "jefe-agent-a:0\njefe-agent-a:jefe-shell\njefe-agent-b:bash\njefe-agent-c:jefe-shell";
    let sessions = parse_sessions_with_shell_windows(raw);
    assert_eq!(
        sessions,
        vec!["jefe-agent-a".to_owned(), "jefe-agent-c".to_owned()],
        "must return only sessions owning jefe-shell, in sorted order"
    );
}

#[test]
fn parse_sessions_deduplicates_multiple_shell_windows_per_session() {
    let raw = "jefe-agent-a:jefe-shell\njefe-agent-a:jefe-shell";
    let sessions = parse_sessions_with_shell_windows(raw);
    assert_eq!(sessions, vec!["jefe-agent-a".to_owned()]);
}

#[test]
fn parse_sessions_returns_empty_when_no_jefe_shell() {
    let raw = "jefe-agent-a:bash\njefe-agent-b:vim";
    assert!(parse_sessions_with_shell_windows(raw).is_empty());
}

#[test]
fn parse_sessions_ignores_blank_and_malformed_lines() {
    let raw = "\n\njefe-agent-a:jefe-shell\n::\nno-colon\n";
    let sessions = parse_sessions_with_shell_windows(raw);
    assert_eq!(sessions, vec!["jefe-agent-a".to_owned()]);
}

#[test]
fn parse_sessions_uses_last_colon_as_delimiter() {
    // A session name containing a colon (rare) must still parse: the window
    // name is the trailing segment after the last colon.
    let raw = "weird:session:jefe-shell";
    let sessions = parse_sessions_with_shell_windows(raw);
    assert_eq!(sessions, vec!["weird:session".to_owned()]);
}

// ---------------------------------------------------------------------------
// Pure parsing: parse_shell_window_names (fallback per-session format)
// ---------------------------------------------------------------------------

#[test]
fn parse_shell_window_names_detects_jefe_shell_membership() {
    assert!(parse_shell_window_names(
        "jefe-shell\nbash\njefe-shell\nvim"
    ));
}

#[test]
fn parse_shell_window_names_absent_when_no_jefe_shell() {
    assert!(!parse_shell_window_names("bash\nvim\nzsh"));
}

#[test]
fn parse_shell_window_names_trims_whitespace() {
    assert!(parse_shell_window_names("  jefe-shell  \n bash "));
}

// ---------------------------------------------------------------------------
// close-all shutdown command plan
// ---------------------------------------------------------------------------

#[test]
fn close_all_shell_windows_plan_targets_jefe_shell_per_session() {
    let multiplexer = plan(LocalPlatform::Unix);
    let session_names = vec!["jefe-agent-a".to_owned(), "jefe-agent-b".to_owned()];
    let commands = close_all_shell_windows_plan(&multiplexer, &session_names);
    assert_eq!(
        commands.len(),
        session_names.len(),
        "one kill-window command per session"
    );
    for (command, session) in commands.iter().zip(session_names.iter()) {
        let args = command.get_args().collect::<Vec<_>>();
        assert!(args.contains(&OsStr::new("kill-window")));
        let expected_target = format!("{session}:{SHELL_WINDOW_NAME}");
        assert!(
            args.windows(2)
                .any(|pair| pair == [OsStr::new("-t"), OsStr::new(&expected_target)]),
            "kill-window must target {expected_target}, got args: {args:?}"
        );
    }
}

#[test]
fn close_all_shell_windows_plan_is_empty_for_no_sessions() {
    let multiplexer = plan(LocalPlatform::Unix);
    assert!(close_all_shell_windows_plan(&multiplexer, &[]).is_empty());
}

#[test]
fn close_all_shell_windows_plan_carries_psmux_namespace() {
    let multiplexer = plan(LocalPlatform::Windows);
    let commands = close_all_shell_windows_plan(&multiplexer, &["jefe-agent".to_owned()]);
    let args = commands[0].get_args().collect::<Vec<_>>();
    assert!(
        args.contains(&OsStr::new("-L")),
        "psmux shutdown must carry -L namespace, got args: {args:?}"
    );
}

// ---------------------------------------------------------------------------
// new-window command shape (open path)
// ---------------------------------------------------------------------------

#[test]
fn new_window_command_for_unix_includes_work_dir_and_env_scrub() {
    let work_dir = Path::new("/tmp/work dir");
    let shell = OsString::from("/bin/bash");
    let command = new_window_command(&plan(LocalPlatform::Unix), "jefe-agent", work_dir, &shell)
        .unwrap_or_else(|error| panic!("new-window command: {error}"));
    let args = command.get_args().collect::<Vec<_>>();
    assert!(
        args.windows(2)
            .any(|pair| pair == [OsStr::new("-c"), work_dir.as_os_str()])
    );
    assert!(args.contains(&shell.as_os_str()));
    assert!(args.contains(&OsStr::new(SHELL_WINDOW_NAME)));
}
