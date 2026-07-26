//! Tmux option passthrough enforcement for [`TmuxRuntimeManager`].
//!
//! Split out of `manager.rs` to keep that file within the source-size gate.
//! Each helper is memoized per session so the tmux option commands run at
//! most once per session across create and attach cycles.

use super::commands;
use super::manager::TmuxRuntimeManager;
use tracing::debug;

impl TmuxRuntimeManager {
    /// Enforce clipboard passthrough for `session_name` if not already done.
    ///
    /// Memoized per session name so the tmux option commands run at most once
    /// per session across create + attach cycles.
    pub(super) fn ensure_clipboard_passthrough(&mut self, session_name: &str) {
        if !self.clipboard_enforced.contains(session_name) {
            commands::enforce_clipboard_passthrough(session_name);
            self.clipboard_enforced.insert(session_name.to_owned());
        }
    }

    /// Test-only accessor: whether clipboard passthrough was already recorded
    /// for `session_name`.
    #[cfg(test)]
    pub(super) fn clipboard_passthrough_enforced(&self, session: &str) -> bool {
        self.clipboard_enforced.contains(session)
    }

    /// Test-only setter for recording clipboard passthrough without invoking tmux.
    #[cfg(test)]
    pub(super) fn record_clipboard_passthrough(&mut self, session: &str) {
        self.clipboard_enforced.insert(session.to_owned());
    }

    /// Enforce tmux prefix passthrough for `session_name` if not already done.
    ///
    /// Memoized per session name so the tmux option commands run at most once
    /// per session across create + attach cycles, mirroring
    /// [`ensure_clipboard_passthrough`](Self::ensure_clipboard_passthrough).
    ///
    /// This is the reattach-side remediation for issue #200: a session created
    /// before the prefix-disabling fix still has tmux's default `C-b` prefix,
    /// which the attach client would use to eat the `0x02` byte of application
    /// control chords. Calling this on every attach guarantees the prefix is
    /// disabled even for pre-existing sessions.
    pub(super) fn ensure_prefix_passthrough(&mut self, session_name: &str) {
        if self.prefix_enforced.contains(session_name) {
            return;
        }
        // Only memoize on success, mirroring the remote path: a transient tmux
        // failure leaves the session un-remediated and un-memoized so the next
        // attach retries (#200 review).
        match commands::configure_prefix_for_passthrough(session_name) {
            Ok(()) => {
                self.prefix_enforced.insert(session_name.to_owned());
            }
            Err(error) => {
                debug!(session_name = %session_name, error = %error, "prefix passthrough failed on local attach; will retry next attach");
            }
        }
    }

    /// Enforce tmux prefix passthrough on a remote session if not already done.
    ///
    /// Remote mirror of [`ensure_prefix_passthrough`](Self::ensure_prefix_passthrough):
    /// best-effort because a transient SSH failure must not block reattach, but
    /// success is memoized so the option is applied exactly once per session.
    pub(super) fn ensure_remote_prefix_passthrough(
        &mut self,
        remote: &crate::domain::RemoteRepositorySettings,
        session_name: &str,
    ) {
        if self.prefix_enforced.contains(session_name) {
            return;
        }
        let command = commands::remote_disable_prefix_command(remote, session_name);
        // run_remote_ssh returns Ok(Output) whenever SSH ran to completion — a
        // non-zero remote exit (session gone, set-option rejected, sudo denied)
        // must NOT be memoized as enforced, or future attaches skip the retry.
        match commands::run_remote_ssh(remote, &command) {
            Ok(output) if output.status.success() => {
                self.prefix_enforced.insert(session_name.to_owned());
            }
            Ok(output) => {
                debug!(
                    session_name = %session_name,
                    status = %output.status,
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "remote prefix passthrough command exited non-zero; will retry next attach"
                );
            }
            Err(error) => {
                debug!(session_name = %session_name, error = %error, "remote prefix passthrough failed on attach; will retry next attach");
            }
        }
    }

    /// Test-only accessor: whether prefix passthrough was already recorded
    /// for `session_name`.
    #[cfg(test)]
    pub(super) fn prefix_passthrough_enforced(&self, session: &str) -> bool {
        self.prefix_enforced.contains(session)
    }

    /// Test-only setter for recording prefix passthrough without invoking tmux.
    #[cfg(test)]
    pub(super) fn record_prefix_passthrough(&mut self, session: &str) {
        self.prefix_enforced.insert(session.to_owned());
    }
}
