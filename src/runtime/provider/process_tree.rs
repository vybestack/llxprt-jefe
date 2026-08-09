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

#[cfg(unix)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

/// The spawned provider process plus its owned standard streams.
pub(super) struct ProviderProcess {
    child: Child,
    known_descendants: BTreeSet<u32>,
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
        let group_result = kill_process_tree(&mut self.child);
        let descendants_result = signal_descendants(&self.known_descendants, "-KILL");
        group_result.and(descendants_result)
    }

    /// Snapshot descendants while the provider leader still proves their
    /// ownership. Escaped process-group members remain in this set after the
    /// leader exits, allowing later shutdown stages to target exact PIDs rather
    /// than broad process names or a potentially recycled leader group.
    pub(super) fn observe_descendants(&mut self) -> io::Result<()> {
        let discovered = descendant_pids(self.child.id(), &self.known_descendants)?;
        self.known_descendants.extend(discovered);
        Ok(())
    }

    /// Gracefully terminate every exact descendant PID observed while the
    /// provider leader still owned it.
    pub(super) fn terminate_descendants(&self) -> io::Result<()> {
        signal_descendants(&self.known_descendants, "-TERM")
    }

    /// Force-kill every exact descendant PID observed while the leader owned it.
    pub(super) fn force_kill_descendants(&self) -> io::Result<()> {
        signal_descendants(&self.known_descendants, "-KILL")
    }

    /// Whether any observed descendant remains alive.
    pub(super) fn descendants_alive(&self) -> bool {
        self.known_descendants.iter().copied().any(process_is_alive)
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
    Ok((
        ProviderProcess {
            child,
            known_descendants: BTreeSet::new(),
        },
        stdin,
        stdout,
        stderr,
    ))
}

/// Stage B: close new requests are already sealed by the caller; signal the
/// process group to terminate gracefully.
///
/// # Errors
///
/// Returns the underlying spawn/exec error if the terminate signal could not
/// be issued. A non-zero command exit (the group already exited) is not an
/// A process group this supervisor is willing to signal.
///
/// The group is named by a number that goes straight into `kill -TERM -<n>`,
/// and two values there are catastrophic: `-0` means the **caller's own**
/// process group, and `-1` means init's. Verified on Linux (procps-ng 4.0.2):
/// `kill -TERM -0` terminated the calling shell, which exited 143 — exactly
/// what a CI runner reports when it is killed mid-job.
///
/// Constructing the group is therefore the place the value is checked, so no
/// caller can pass a bare integer and hope it names a child (issue #390).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessGroup(u32);

impl ProcessGroup {
    /// The group a spawned child leads, or `None` if the pid cannot name one.
    ///
    /// Providers are spawned with `process_group(0)`, so a live child's pid is
    /// its own group id.
    pub(super) const fn of_child(pid: u32) -> Option<Self> {
        if pid > 1 { Some(Self(pid)) } else { None }
    }

    /// The group id.
    pub(super) const fn id(self) -> u32 {
        self.0
    }
}

/// The error returned when a pid cannot safely name a process group.
fn refused_group(pid: u32) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "refusing to signal process group {pid}: it does not name a provider child \
             (0 is our own group, 1 is init)"
        ),
    )
}

/// error here.
pub(super) fn terminate_process_tree(pid: u32) -> io::Result<()> {
    let Some(group_id) = ProcessGroup::of_child(pid) else {
        return Err(refused_group(pid));
    };
    #[cfg(unix)]
    {
        let group = format!("-{}", group_id.id());
        let args = unix_group_signal_args("-TERM", group.as_str());
        Command::new("kill")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(windows)]
    {
        let pid_text = group_id.id().to_string();
        Command::new("taskkill")
            .args(["/PID", pid_text.as_str(), "/T"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = group_id;
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
    let Some(group_id) = ProcessGroup::of_child(pid) else {
        return Err(refused_group(pid));
    };
    #[cfg(unix)]
    {
        let group = format!("-{}", group_id.id());
        let args = unix_group_signal_args("-KILL", group.as_str());
        Command::new("kill")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(windows)]
    {
        let pid_text = group_id.id().to_string();
        Command::new("taskkill")
            .args(["/PID", pid_text.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = group_id;
    }
    Ok(())
}

#[cfg(unix)]
fn descendant_pids(root: u32, retained: &BTreeSet<u32>) -> io::Result<BTreeSet<u32>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("could not enumerate provider descendants"));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("process listing was not UTF-8"))?;
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.split_ascii_whitespace();
        let Some(pid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some(parent) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        if pid > 1 {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut discovered = retained.clone();
    let mut frontier = vec![root];
    frontier.extend(retained.iter().copied());
    while let Some(parent) = frontier.pop() {
        if let Some(direct) = children.get(&parent) {
            for pid in direct {
                if discovered.insert(*pid) {
                    frontier.push(*pid);
                }
            }
        }
    }
    discovered.remove(&root);
    Ok(discovered)
}

#[cfg(not(unix))]
fn descendant_pids(root: u32, retained: &BTreeSet<u32>) -> io::Result<BTreeSet<u32>> {
    if root == 0 || retained.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider process identities must be positive",
        ));
    }
    Ok(retained.clone())
}

#[cfg(unix)]
fn signal_descendants(descendants: &BTreeSet<u32>, signal: &str) -> io::Result<()> {
    for pid in descendants {
        let pid_text = pid.to_string();
        Command::new("kill")
            .args([signal, pid_text.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn signal_descendants(descendants: &BTreeSet<u32>, _signal: &str) -> io::Result<()> {
    if descendants.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider descendant identities must be positive",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    let pid_text = pid.to_string();
    let Ok(output) = Command::new("ps")
        .args(["-o", "stat=", "-p", pid_text.as_str()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return true;
    };
    if !output.status.success() {
        return false;
    }
    let state = String::from_utf8_lossy(&output.stdout);
    state
        .split_ascii_whitespace()
        .next()
        .is_some_and(|value| !value.starts_with('Z'))
}

#[cfg(not(unix))]
const fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
const fn unix_group_signal_args<'a>(signal: &'a str, group: &'a str) -> [&'a str; 3] {
    [signal, "--", group]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `kill -TERM -0` targets the **caller's own** process group. Verified on
    /// Linux (procps-ng 4.0.2) inside a container: it terminated the calling
    /// shell, which exited 143 — the same signature a CI runner reports when it
    /// is killed mid-job. A pid of 1 is init's group. Neither may ever reach the
    /// signal, so they are refused where the group is named rather than trusted
    /// to be a real child (issue #390).
    #[test]
    fn a_process_group_is_never_built_from_a_pid_that_would_signal_ourselves() {
        assert!(
            ProcessGroup::of_child(0).is_none(),
            "pid 0 names the caller's own process group"
        );
        assert!(
            ProcessGroup::of_child(1).is_none(),
            "pid 1 names init's process group"
        );
    }

    #[test]
    fn a_real_child_pid_yields_its_own_group() {
        let group = ProcessGroup::of_child(4242);
        assert_eq!(group.map(ProcessGroup::id), Some(4242));
    }

    /// Signalling a refused group must be a typed error the caller can report,
    /// not a silent no-op that looks like a successful reap.
    #[test]
    fn signalling_a_refused_group_is_an_error() {
        for pid in [0, 1] {
            assert!(
                terminate_process_tree(pid).is_err(),
                "terminate must refuse pid {pid}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_group_signal_uses_an_option_delimiter_before_the_negative_pid() {
        assert_eq!(
            unix_group_signal_args("-TERM", "-4242"),
            ["-TERM", "--", "-4242"],
            "a negative process-group id must not be parsed as a kill option or signal"
        );
        assert_eq!(
            unix_group_signal_args("-KILL", "-4242"),
            ["-KILL", "--", "-4242"]
        );
    }
}
