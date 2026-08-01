//! Pure server-health classification for runtime reconciliation.
//!
//! Side-effect-free comparison of persisted and freshly observed server
//! identities. Callers own all I/O; this module only translates identity
//! transitions into a health verdict so reconciliation logic stays
//! deterministic and testable.

use crate::domain::ServerProcessIdentity;
use crate::runtime::MultiplexerVersion;
/// Composite identity of one runtime server instance: the operating-system
/// process plus the multiplexer version hosting it. Two identities are the
/// same server only when both components agree.
///
/// The process component is a [`ServerProcessIdentity`], so the multiplexer
/// server can never be compared against, or substituted for, a pane leader or
/// an agent worker (issue #543).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerIdentity {
    pub process: ServerProcessIdentity,
    pub multiplexer: MultiplexerVersion,
}

impl ServerIdentity {
    #[must_use]
    pub const fn new(process: ServerProcessIdentity, multiplexer: MultiplexerVersion) -> Self {
        Self {
            process,
            multiplexer,
        }
    }
}

/// Health verdict produced by comparing a previous and current server
/// identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerHealth {
    /// The server is present and matches the previously tracked identity.
    Healthy,
    /// No live server remains. A server that disappears while agents are still
    /// tracked is treated as lost.
    Gone,
    /// A different process now occupies the server role: either the PID or its
    /// creation token changed since the prior observation.
    Replaced,
}

/// Classify the health of the runtime server from its identity transition.
///
/// The `has_tracked_agents` flag only disambiguates the doubly-absent case
/// (`previous` and `current` both `None`): with tracked agents a missing server
/// is `Gone`, otherwise the absence is an idle baseline and `Healthy`. In every
/// other transition the identity evidence is decisive.
#[must_use]
pub fn classify_server_health(
    previous: Option<&ServerIdentity>,
    current: Option<&ServerIdentity>,
    has_tracked_agents: bool,
) -> ServerHealth {
    match (previous, current) {
        (None, None) => {
            if has_tracked_agents {
                ServerHealth::Gone
            } else {
                ServerHealth::Healthy
            }
        }
        (None, Some(_)) => ServerHealth::Healthy,
        (Some(_), None) => ServerHealth::Gone,
        (Some(previous), Some(current)) => {
            // A changed PID or a changed creation token both indicate a
            // distinct process instance now occupying the server role.
            if previous.process.pid() == current.process.pid()
                && previous.process.started_at() == current.process.started_at()
            {
                ServerHealth::Healthy
            } else {
                ServerHealth::Replaced
            }
        }
    }
}

/// Raw evidence captured by one batch liveness probe of the multiplexer
/// server (issue #493 Stack A).
///
/// The caller performs the single `display-message -p '#{pid}|#{version}'`
/// subprocess invocation and records what happened; this pure type carries
/// that record into [`classify_server_liveness`] so the classification has no
/// I/O and is fully testable. All `stderr`/`stdout` strings are diagnostic
/// payloads owned by the runtime boundary and never persist unredacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerLivenessEvidence {
    /// The probe command completed. `stdout` is the raw format output and
    /// `stderr` any trailing diagnostic.
    CommandCompleted { stdout: String, stderr: String },
    /// The probe command exited nonzero. `stderr` is the raw diagnostic used
    /// only to distinguish a missing server from an unrelated failure.
    CommandFailed { stderr: String },
    /// The probe command could not be spawned or did not complete in time.
    SpawnFailed,
}

impl ServerLivenessEvidence {
    /// Construct evidence for a successful command capturing `stdout`/`stderr`.
    #[must_use]
    pub fn command_succeeded(stdout: &str, stderr: &str) -> Self {
        Self::CommandCompleted {
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    /// Construct evidence for a nonzero command capturing `stderr`.
    #[must_use]
    pub fn command_failed(stderr: &str) -> Self {
        Self::CommandFailed {
            stderr: stderr.to_owned(),
        }
    }

    /// Construct evidence for a spawn/timeout failure.
    #[must_use]
    pub const fn spawn_failed() -> Self {
        Self::SpawnFailed
    }
}

/// Health verdict produced by one batch liveness probe, paired with the
/// freshly observed identity when the server is present (issue #493 Stack A).
///
/// This extends [`ServerHealth`] with the explicit `Unavailable` outcome so
/// the caller can distinguish "no state change" (Unavailable) from "server is
/// gone/replaced" (Gone/Replaced). The carried identity lets the caller update
/// its tracked server handle on Healthy/Replaced without re-probing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerLivenessObservation {
    /// The server is present. Carries the identity established by this probe.
    Healthy(Option<ServerIdentity>),
    /// No live server remains. Existing tracked agents should transition to
    /// [`crate::domain::AgentStatus::ServerLost`].
    Gone,
    /// A different process now occupies the server role. Same recovery action
    /// as `Gone`, but the caller also knows the prior server was replaced.
    Replaced(ServerIdentity),
    /// The probe could not establish a verdict (spawn failure, timeout,
    /// malformed output, or a nonzero command that does not indicate a
    /// missing server). The caller must make no state change.
    Unavailable,
}

/// Lowercase substrings that, when present in a failed probe's stderr,
/// indicate the multiplexer server is genuinely absent rather than
/// temporarily unreachable. Matched case-insensitively so psmux/tmux wording
/// differences do not leak through.
const NO_SERVER_STDERR_MARKERS: &[&str] = &[
    "no server running",
    "no sessions",
    "server not found",
    "failed to connect to server",
    "error connecting to",
];

/// Pure classification of one batch liveness probe's evidence against the
/// previously tracked server identity (issue #493 Stack A).
///
/// - A successful command with parseable, matching identity is `Healthy`.
/// - A successful command with a parseable but changed identity is `Replaced`.
/// - A successful command with unparseable output fails open as `Unavailable`.
/// - A nonzero command whose stderr indicates a missing server is `Gone`.
/// - A nonzero command with unrelated stderr fails open as `Unavailable`.
/// - A spawn/timeout failure fails open as `Unavailable`.
///
/// `prior` is `None` on the first observation after startup. Probing occurs
/// only while local agents are tracked, so an explicit missing-server signal is
/// `Gone` even without a prior baseline; a successful probe establishes that
/// baseline as `Healthy`.
#[must_use]
pub fn classify_server_liveness(
    prior: Option<&ServerIdentity>,
    evidence: &ServerLivenessEvidence,
) -> ServerLivenessObservation {
    match evidence {
        ServerLivenessEvidence::CommandCompleted { stdout, .. } => {
            match parse_server_identity_output(stdout) {
                Some(current) => {
                    let health = classify_server_health(prior, Some(&current), true);
                    match health {
                        ServerHealth::Healthy => ServerLivenessObservation::Healthy(Some(current)),
                        ServerHealth::Replaced => ServerLivenessObservation::Replaced(current),
                        // classify_server_health only returns Gone when
                        // current is None, which cannot happen here.
                        ServerHealth::Gone => ServerLivenessObservation::Unavailable,
                    }
                }
                None => ServerLivenessObservation::Unavailable,
            }
        }
        ServerLivenessEvidence::CommandFailed { stderr } => {
            if stderr_indicates_no_server(stderr) {
                ServerLivenessObservation::Gone
            } else {
                ServerLivenessObservation::Unavailable
            }
        }
        ServerLivenessEvidence::SpawnFailed => ServerLivenessObservation::Unavailable,
    }
}

/// Return whether a failed probe's stderr indicates the server is absent.
///
/// Case-insensitive substring match against a small, reviewed marker list so
/// arbitrary stderr text is never treated as authoritative.
#[must_use]
fn stderr_indicates_no_server(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    NO_SERVER_STDERR_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Parse the `display-message -p '#{pid}|#{version}'` output of one server
/// probe into a [`ServerIdentity`] (issue #493 Stack A).
///
/// Expected form: `<pid>|<major>.<minor>.<patch>` (e.g. `4321|3.3.7`). The
/// PID supplies the process identity; `started_at` defaults to `1` because
/// the multiplexer `display-message` format string does not expose a creation
/// token, and the probe distinguishes server replacement via a PID change
/// rather than a creation-token change (the per-agent
/// [`crate::domain::ProcessIdentity`] service remains the authoritative
/// reuse-safe check).
///
/// Returns `None` for any malformed input so the caller fails open.
#[must_use]
pub fn parse_server_identity_output(output: &str) -> Option<ServerIdentity> {
    let trimmed = output.trim();
    let (pid_raw, version_raw) = trimmed.split_once('|')?;
    let pid: u32 = pid_raw.trim().parse().ok()?;
    let version = MultiplexerVersion::parse(version_raw.trim()).ok()?;
    Some(ServerIdentity::new(
        ServerProcessIdentity::new(pid, 1),
        version,
    ))
}
