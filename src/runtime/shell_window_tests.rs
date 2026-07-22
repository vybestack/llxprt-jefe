//! Structural tests for embedded shell-window command construction.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::*;
use crate::runtime::multiplexer::{LocalPlatform, MultiplexerIsolation};

fn plan(platform: LocalPlatform) -> MultiplexerPlan {
    let executable = if platform == LocalPlatform::Windows {
        PathBuf::from("psmux.exe")
    } else {
        PathBuf::from("tmux")
    };
    let isolation = if platform == LocalPlatform::Windows {
        MultiplexerIsolation::Namespace("jefe-test-shell-window".to_owned())
    } else {
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe shell.sock"))
    };
    MultiplexerPlan::for_platform(platform, executable, isolation)
        .unwrap_or_else(|error| panic!("test plan: {error}"))
}

#[test]
fn new_window_uses_exact_work_dir_and_structural_unix_shell_path() {
    let work_dir = Path::new("/tmp/work dir Ω");
    let shell = OsString::from("/tmp/shell path;not-code");
    let command = new_window_command(&plan(LocalPlatform::Unix), "jefe-agent", work_dir, &shell)
        .unwrap_or_else(|error| panic!("new-window command: {error}"));
    let args = command.get_args().collect::<Vec<_>>();
    assert!(
        args.windows(2)
            .any(|pair| pair == [OsStr::new("-c"), work_dir.as_os_str()])
    );
    assert!(args.contains(&shell.as_os_str()));
    assert!(args.iter().any(|arg| *arg == OsStr::new("TMUX_TMPDIR")));
}

#[test]
fn new_window_uses_powershell_environment_scrub_for_psmux() {
    let command = new_window_command(
        &plan(LocalPlatform::Windows),
        "jefe-agent",
        Path::new(r"C:\work dir"),
        &OsString::from(r"C:\Program Files\PowerShell\pwsh.exe"),
    )
    .unwrap_or_else(|error| panic!("new-window command: {error}"));
    let joined = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("$env:TMUX=$null"),
        "missing psmux scrub: {joined}"
    );
    assert!(
        !joined.contains("env -u"),
        "Unix wrapper leaked to psmux: {joined}"
    );
}

#[test]
fn shell_window_name_is_stable() {
    assert_eq!(SHELL_WINDOW_NAME, "jefe-shell");
}

#[test]
fn preview_targets_the_shell_window_for_unix_and_psmux() {
    for platform in [LocalPlatform::Unix, LocalPlatform::Windows] {
        let command = capture_shell_preview_command(&plan(platform), "jefe-agent-42");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            args.windows(2)
                .any(|pair| { pair == ["-t".to_owned(), "jefe-agent-42:jefe-shell".to_owned()] })
        );
    }
}
