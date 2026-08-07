//! Cross-platform provider process spawn and tree termination
//! (issue #390 CW-10, CW10-11).
//!
//! The supervisor is the sole owner of the provider [`Child`], its process
//! group, and its descendants. This private helper centralises the platform
//! plumbing — process-group creation on Unix and `CREATE_NEW_PROCESS_GROUP` on
//! Windows for spawn, and escalating tree termination for shutdown — so the
//! supervisor body reads as one lifecycle. The patterns mirror the existing
//! `command_capture.rs` / `process.rs` conventions; no new dependency or
//! `unsafe` is introduced, and native Windows compiles.

use std::io;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

/// The spawned provider process plus its owned standard streams.
pub(super) struct ProviderProcess {
    child: Child,
}

impl ProviderProcess {
    /// The operating-system process identifier (also the process-group id on
    /// Unix, where the child leads a fresh group).
    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Probe for exit without blocking.
    ///
    /// # Errors
    ///
    /// Forwards the underlying wait error.
    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    /// Stage C helper: issue force-kill signals to the whole process tree
    /// without blocking on a wait. Reaping is the caller's bounded
    /// responsibility (see [`ProviderProcess::try_wait`]).
    ///
    /// # Errors
    ///
    /// Forwards the underlying signal-delivery error if neither the group
    /// signal nor the in-process `kill` could be issued.
    pub(super) fn force_kill_tree(&mut self) -> io::Result<()> {
        kill_process_tree(&mut self.child)
    }
}

/// Spawn a provider process from a fully-configured command, attaching piped
/// standard streams and placing it in its own process group.
///
/// # Errors
///
/// Forwards the spawn error, or returns a synthetic I/O error when the platform
/// did not expose a piped stream as expected.
pub(super) fn spawn(
    mut command: Command,
) -> io::Result<(ProviderProcess, ChildStdin, ChildStdout, ChildStderr)> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_tree(&mut command);
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("provider did not expose stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("provider did not expose stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("provider did not expose stderr"))?;
    Ok((ProviderProcess { child }, stdin, stdout, stderr))
}

/// Stage B: close new requests are already sealed by the caller; signal the
/// process group to terminate gracefully.
///
/// # Errors
///
/// Returns the underlying spawn/exec error if the terminate signal could not
/// be issued. A non-zero command exit (the group already exited) is not an
/// error here.
pub(super) fn terminate_process_tree(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        let group = format!("-{pid}");
        Command::new("kill")
            .args(["-TERM", group.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(windows)]
    {
        let pid_text = pid.to_string();
        Command::new("taskkill")
            .args(["/PID", pid_text.as_str(), "/T"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
    Ok(())
}

/// Stage C: issue force-kill signals to the whole process tree without
/// blocking on a wait. The group signal targets descendants; the in-process
/// `kill` targets the leader. Reaping is the caller's bounded responsibility:
/// this function never calls `wait`, so it cannot block unbounded.
///
/// # Errors
///
/// Returns the group-signal error if it could not be issued, otherwise the
/// in-process kill's error. `child.kill()` is attempted unconditionally, even
/// when the group signal fails, because the leader must not be left running
/// merely because the external `kill` binary was unavailable. A non-zero
/// command exit (the group already exited) is not an error here.
pub(super) fn kill_process_tree(child: &mut Child) -> io::Result<()> {
    // The group signal's error is captured rather than propagated: `?` here
    // would skip the in-process kill below, so a missing or unrunnable `kill`
    // binary would leave the provider leader alive. Killing the leader is the
    // one thing this function must always attempt.
    let group_result = kill_process_group(child.id());
    let leader_result = child.kill();
    group_result.and(leader_result)
}

/// Force-signal the child's whole process group, targeting its descendants.
///
/// A non-zero exit means the group has already gone, which is not an error.
fn kill_process_group(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        let group = format!("-{pid}");
        Command::new("kill")
            .args(["-KILL", group.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(windows)]
    {
        let pid_text = pid.to_string();
        Command::new("taskkill")
            .args(["/PID", pid_text.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
    Ok(())
}

#[cfg(unix)]
fn configure_process_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // A fresh process group whose id equals the child pid, so the supervisor can
    // signal the whole tree (provider plus non-detached descendants) by group.
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_tree(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP: the provider runs in its own console group
        // so it does not share the host's control events. Tree termination uses
        // `taskkill /T` (the repository Windows containment convention).
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}
