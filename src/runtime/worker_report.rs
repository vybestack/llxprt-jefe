//! Session-host → jefe report of the agent worker's process identity.
//!
//! Where the pane leader is not the agent (Windows: `pwsh` → session host →
//! agent), jefe cannot learn the worker's identity by asking the multiplexer:
//! `#{pane_pid}` answers about the pane leader, two hops above the worker. The
//! session host is the only process that observes the worker's PID at the
//! moment it is created, so it records that observation here and jefe reads it
//! back (issue #543).
//!
//! The report is evidence, not authority: a missing or malformed report leaves
//! the worker identity *unknown*, and an unknown worker identity is never
//! substituted with the pane leader's.

use std::path::{Path, PathBuf};

use crate::domain::{PaneProcessIdentity, WorkerProcessIdentity};

/// Filename prefix for worker reports written into the system temp directory.
const REPORT_PREFIX: &str = "jefe-worker-report-";

/// One session host's observation of the worker it spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkerReport {
    /// PID of the session host that spawned the worker.
    pub host_pid: u32,
    /// PID of the agent worker itself.
    pub worker_pid: u32,
    /// Creation token of the worker, when the host could capture one. `None`
    /// means the identity cannot be reuse-checked and callers must treat it as
    /// weak evidence.
    #[serde(default)]
    pub worker_started_at: Option<u64>,
}

impl WorkerReport {
    /// Project the report into a worker identity.
    ///
    /// A zero PID is rejected: it is never a real process, and accepting it
    /// would let a malformed report masquerade as a live worker.
    #[must_use]
    pub fn worker_identity(&self) -> Option<WorkerProcessIdentity> {
        if self.worker_pid == 0 {
            return None;
        }
        Some(match self.worker_started_at {
            Some(started_at) => WorkerProcessIdentity::new(self.worker_pid, started_at),
            None => WorkerProcessIdentity::from_pid(self.worker_pid),
        })
    }

    /// Whether this report describes a host that spawned a *distinct* worker.
    ///
    /// A report claiming the worker and host are the same process is not
    /// evidence of a worker below the host; it is a host that failed to
    /// distinguish them, which is exactly the conflation issue #543 removes.
    #[must_use]
    pub const fn describes_distinct_worker(&self) -> bool {
        self.worker_pid != 0 && self.worker_pid != self.host_pid
    }
}

/// Deterministic report path for one session, derived from its name.
///
/// Both jefe and the session host compute this independently, so no path needs
/// to survive a round trip through the multiplexer command line.
#[must_use]
pub fn report_path_for_session(session_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{REPORT_PREFIX}{}.json", sanitize(session_name)))
}

/// Reduce a session name to characters that are safe in a filename on every
/// supported platform, without ever merging two names onto one file.
///
/// Collapsing every unsafe character to `_` would not be injective: `a/b` and
/// `a_b` would share a report, letting one agent adopt the worker identity a
/// different agent's host recorded. That is the misattribution issue #543
/// exists to remove, so the escape is reversible instead: `_` doubles, and any
/// other unsafe character becomes its code point between underscores. Distinct
/// session names therefore always yield distinct paths.
fn sanitize(session_name: &str) -> String {
    use std::fmt::Write as _;

    let mut sanitized = String::with_capacity(session_name.len());
    for character in session_name.chars() {
        if character.is_ascii_alphanumeric() || character == '-' {
            sanitized.push(character);
        } else if character == '_' {
            sanitized.push_str("__");
        } else {
            // Ignoring the result is sound: writing into a String cannot fail.
            let _ = write!(sanitized, "_{:x}_", character as u32);
        }
    }
    sanitized
}

/// Record a host's worker observation. Best effort: a failure to write leaves
/// the worker identity unknown rather than wrong, so it is logged and ignored.
pub fn write_report(path: &Path, report: &WorkerReport) {
    match serde_json::to_vec(report) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(path, bytes) {
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "could not record the agent worker identity; it will remain unknown"
                );
            }
        }
        Err(error) => tracing::warn!(
            error = %error,
            "could not serialize the agent worker identity report"
        ),
    }
}

/// Read a host's worker observation, if one has been recorded and parses.
#[must_use]
pub fn read_report(path: &Path) -> Option<WorkerReport> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Remove a consumed report. Best effort; a leftover file is harmless because
/// the next launch overwrites it.
pub fn remove_report(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Resolve the worker identity for a session from its host's report.
///
/// Returns `None` — meaning *unknown* — when no report exists, the report is
/// malformed, or the report does not describe a worker distinct from its host.
/// The pane identity is supplied only so the caller's intent is explicit at the
/// call site; it is deliberately never used as a fallback answer (issue #543).
#[must_use]
pub fn worker_identity_from_report(
    session_name: &str,
    _pane: PaneProcessIdentity,
) -> Option<WorkerProcessIdentity> {
    let report = read_report(&report_path_for_session(session_name))?;
    if !report.describes_distinct_worker() {
        return None;
    }
    report.worker_identity()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_path_is_stable_for_one_session_and_distinct_across_sessions() {
        let first = report_path_for_session("jefe-agent-alpha");
        let again = report_path_for_session("jefe-agent-alpha");
        let other = report_path_for_session("jefe-agent-beta");

        assert_eq!(first, again, "the path must be derivable, not generated");
        assert_ne!(first, other, "distinct sessions must not share a report");
    }

    /// Sanitizing for the filesystem must not merge two different sessions onto
    /// one report file. If it did, an agent could adopt the worker identity a
    /// different agent's host recorded -- the same misattribution issue #543
    /// exists to remove, reintroduced one layer down.
    #[test]
    fn sessions_that_sanitize_alike_still_get_separate_report_paths() {
        let slashed = report_path_for_session("jefe/agent");
        let underscored = report_path_for_session("jefe_agent");

        assert_ne!(
            slashed, underscored,
            "distinct session names must not share a worker report file"
        );
    }

    #[test]
    fn report_path_neutralizes_separators_in_the_session_name() {
        let path = report_path_for_session("../../escape/attempt");
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            panic!("the report path must have a filename");
        };

        assert!(
            !name.contains('/') && !name.contains('\\') && !name.contains(".."),
            "a session name must not be able to steer the report outside temp: {name}"
        );
    }

    #[test]
    fn a_report_naming_its_own_host_is_not_evidence_of_a_worker() {
        let report = WorkerReport {
            host_pid: 4321,
            worker_pid: 4321,
            worker_started_at: Some(99),
        };

        assert!(
            !report.describes_distinct_worker(),
            "host==worker is the conflation this issue removes, not evidence"
        );
    }

    #[test]
    fn a_zero_worker_pid_yields_no_identity() {
        let report = WorkerReport {
            host_pid: 4321,
            worker_pid: 0,
            worker_started_at: Some(99),
        };

        assert!(
            report.worker_identity().is_none(),
            "a zero PID must never project into a usable identity"
        );
    }

    #[test]
    fn a_report_without_a_creation_token_still_identifies_the_worker_weakly() {
        let report = WorkerReport {
            host_pid: 4321,
            worker_pid: 8765,
            worker_started_at: None,
        };

        let Some(identity) = report.worker_identity() else {
            panic!("a non-zero worker pid must project into an identity");
        };
        assert_eq!(identity.pid(), 8765);
        assert_eq!(
            identity.started_at(),
            None,
            "an absent creation token must stay absent, not be invented"
        );
    }

    #[test]
    fn a_missing_report_leaves_the_worker_unknown_rather_than_the_pane() {
        let pane = PaneProcessIdentity::new(1111, 7);
        let resolved = worker_identity_from_report("jefe-session-never-written", pane);

        assert!(
            resolved.is_none(),
            "with no report the worker is unknown; the pane must not stand in for it"
        );
    }

    #[test]
    fn a_recorded_report_resolves_the_worker_below_the_host() {
        let session = format!("jefe-report-roundtrip-{}", std::process::id());
        let path = report_path_for_session(&session);
        let report = WorkerReport {
            host_pid: 2222,
            worker_pid: 3333,
            worker_started_at: Some(42),
        };
        write_report(&path, &report);

        let pane = PaneProcessIdentity::new(2222, 7);
        let resolved = worker_identity_from_report(&session, pane);
        remove_report(&path);

        let Some(identity) = resolved else {
            panic!("a recorded report must resolve the worker identity");
        };
        assert_eq!(identity.pid(), 3333, "the worker, not the pane leader");
        assert_ne!(
            identity.pid(),
            pane.pid(),
            "the resolved worker must be distinct from the pane leader"
        );
    }
}
