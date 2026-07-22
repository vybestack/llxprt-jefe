//! Private-socket-aware signal cleanup for real-tmux harness runs (issue #375).
//!
//! Rust `Drop` is bypassed when the process receives an abrupt signal (SIGINT,
//! SIGTERM, SIGHUP, SIGQUIT). Without this module, a signal during a harness
//! run leaves the harness-owned private-socket tmux server
//! (`-L jefe-harness-<pid>`) and its sessions alive indefinitely.
//!
//! [`SignalCleanupGuard`] registers Unix signal hooks via a background thread
//! (signal-hook's self-pipe mechanism, which is async-signal-safe). When a
//! registered signal is delivered, the background thread kills only the
//! harness-owned tmux server via [`TmuxDriver::kill_harness_server`], then
//! terminates the process so the signal's intent (process death) is honored.
//!
//! On Windows the guard is a zero-sized no-op: the psmux-backed harness has no
//! long-lived server process, and psmux processes die with the parent process.
//!
//! Artifact directories are intentionally never touched — the issue explicitly
//! requires preserving diagnostic artifacts.

use super::tmux_driver::TmuxDriver;

/// Signals that trigger harness tmux server cleanup (issue #375).
///
/// These cover the standard "terminate the process" family on Unix. `SIGKILL`
/// is intentionally excluded because it cannot be caught.
#[cfg(unix)]
const HANDLED_SIGNALS: &[i32] = &[
    signal_hook::consts::SIGINT,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGHUP,
    signal_hook::consts::SIGQUIT,
];

/// RAII guard that kills the harness-owned tmux server on signal delivery
/// (issue #375).
///
/// Construct this guard before starting a tmux session in
/// [`run_tmux_scenario`](super::runner::run_tmux_scenario). The guard
/// spawns a background thread that listens for Unix termination signals
/// (SIGINT, SIGTERM, SIGHUP, SIGQUIT). When a signal arrives, the thread
/// calls [`TmuxDriver::kill_harness_server`] — targeting **only** the
/// harness private socket — and then exits the process so the signal's
/// termination intent is honored.
///
/// On `Drop`, the signal pipe is closed so the background thread exits
/// cleanly and no further signal-triggered cleanup occurs.
///
/// On Windows this is a no-op (psmux processes die with the parent).
#[derive(Debug)]
pub struct SignalCleanupGuard {
    #[cfg(unix)]
    handle: Option<signal_hook::iterator::Handle>,
    _marker: std::marker::PhantomData<()>,
}

impl SignalCleanupGuard {
    /// Register signal handlers that kill the harness tmux server on signal.
    ///
    /// The background thread captures a [`TmuxDriver`] (cheap — it is
    /// zero-sized) and calls `kill_harness_server()` when a registered
    /// signal arrives. Only the harness private socket is affected.
    ///
    /// # Errors
    ///
    /// Returns `Err` on Unix if signal handler registration fails.
    #[cfg(unix)]
    pub fn new(driver: TmuxDriver) -> Result<Self, std::io::Error> {
        let mut signals = signal_hook::iterator::Signals::new(HANDLED_SIGNALS.iter().copied())?;
        let handle = signals.handle();

        let builder = std::thread::Builder::new().name("harness-signal-cleanup".to_string());
        builder.spawn(move || {
            // Block until a registered signal arrives, then clean up and
            // exit. Since `std::process::exit` terminates the process,
            // only the first signal is handled; subsequent signals are
            // irrelevant (the process is already dying).
            if let Some(sig) = signals.forever().next() {
                perform_cleanup(&driver);
                // Honor the signal's termination intent: exit with the
                // conventional 128 + signal_number status so callers
                // (CI, shell scripts) see a signal-death exit code.
                std::process::exit(128 + sig);
            }
        })?;

        Ok(Self {
            handle: Some(handle),
            _marker: std::marker::PhantomData,
        })
    }

    /// Windows no-op constructor.
    #[cfg(not(unix))]
    #[must_use]
    pub fn new(driver: TmuxDriver) -> Result<Self, std::convert::Infallible> {
        // Suppress unused-variable warning; the driver is not needed on
        // Windows (psmux has no persistent server).
        drop(driver);
        Ok(Self {
            _marker: std::marker::PhantomData,
        })
    }

    /// Return the number of signal types monitored by this guard.
    ///
    /// Always 0 on Windows. On Unix, equals the number of signals in
    /// [`HANDLED_SIGNALS`] (currently 4).
    #[must_use]
    pub fn handler_count(&self) -> usize {
        #[cfg(unix)]
        {
            HANDLED_SIGNALS.len()
        }
        #[cfg(not(unix))]
        {
            0
        }
    }
}

/// Kill the harness-owned tmux server. Called by the signal-handler thread
/// and directly testable (issue #375).
///
/// This targets only the harness private socket. It never touches the
/// shared/default tmux server, any other `-L` socket, or unrelated sessions.
/// It never deletes files.
fn perform_cleanup(driver: &TmuxDriver) {
    if let Err(err) = driver.kill_harness_server() {
        tracing::warn!(%err, "harness signal cleanup: failed to kill tmux server");
    }
}

impl Drop for SignalCleanupGuard {
    fn drop(&mut self) {
        // Close the signal pipe so the background thread's `forever()`
        // iterator ends and the thread exits cleanly. This unregisters
        // our interest in the handled signals without affecting other
        // signal handlers that may exist in the process.
        #[cfg(unix)]
        if let Some(handle) = self.handle.take() {
            handle.close();
        }
    }
}

#[cfg(test)]
#[path = "signal_cleanup_tests.rs"]
mod tests;
