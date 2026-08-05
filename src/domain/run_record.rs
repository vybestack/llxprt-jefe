//! Run-boundary records: what a jefe run says about itself while it is alive,
//! and what a later run may conclude from that when the earlier one is gone
//! (issue #662).
//!
//! A process that is terminated externally executes none of its own code on the
//! way out, so nothing it could have written *at* death is available. The only
//! evidence that survives is what it wrote *before* death. A run therefore
//! publishes a [`RunMarker`] while it is alive and removes it on a recorded
//! exit; a marker still present at the next start is the fingerprint of a run
//! that ended without saying why.
//!
//! Presence alone is not proof of a crash, because a second jefe may simply be
//! running. The distinction is made by probing the *recorded owner* rather than
//! the bare PID: [`ProcessIdentity`] pairs the PID with a creation
//! discriminator, so a recycled PID does not masquerade as the original owner.
//! This module holds only the pure decision; the probe itself is performed at
//! the boundary and passed in as a [`PriorRunProbe`].

use serde::{Deserialize, Serialize};

use super::ProcessIdentity;

/// Why a run ended, recorded in the log at the run boundary.
///
/// The point of a closed set is attribution: an operator reading the log can
/// tell a deliberate quit from a render failure from an unwinding panic, and
/// the *absence* of any of these from an external kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEndReason {
    /// The operator quit the application.
    UserQuit,
    /// The terminal or render loop failed and the run could not continue.
    RenderFailed,
    /// The run is unwinding from a panic.
    Panic,
    /// The run ended without any path recording a reason.
    Unknown,
}

impl RunEndReason {
    /// Stable label written to the log.
    ///
    /// These strings are a diagnostic contract: they are what an operator or a
    /// support script greps for, so they do not follow variant renames.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserQuit => "user-quit",
            Self::RenderFailed => "render-failed",
            Self::Panic => "panic",
            Self::Unknown => "unknown",
        }
    }
}

/// The "run in progress" record a live run publishes about itself.
///
/// It is deliberately small. Everything here exists to answer one of three
/// questions after the fact: which process was it, was that process still the
/// same process, and what was it doing when it stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMarker {
    /// Identity of the process that owns this marker.
    pub identity: ProcessIdentity,
    /// Version of jefe that wrote the marker.
    pub version: String,
    /// Wall-clock start of the run, in Unix seconds.
    pub started_unix: u64,
    /// Most recent heartbeat, in Unix seconds. This bounds the moment of death
    /// from below when nothing else recorded it.
    pub last_seen_unix: u64,
    /// The operation in flight at the last heartbeat, when one was recorded.
    #[serde(default)]
    pub breadcrumb: Option<String>,
}

/// What a liveness probe of a prior run's recorded owner concluded.
///
/// Produced at the boundary from the platform process probe. `Indeterminate`
/// is a first-class answer rather than a failure: reporting a crash that did
/// not happen is worse than reporting nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorRunProbe {
    /// The recorded owner is still running.
    OwnerAlive,
    /// The recorded owner is gone, or its PID now belongs to another process.
    OwnerGone,
    /// Liveness could not be established.
    Indeterminate,
}

/// A prior run that ended with no recorded reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncleanRun {
    /// PID of the run that vanished.
    pub pid: u32,
    /// Version of jefe that run was executing.
    pub version: String,
    /// Last heartbeat it published, in Unix seconds.
    pub last_seen_unix: u64,
    /// Operation in flight at that heartbeat, when one was recorded.
    pub breadcrumb: Option<String>,
}

impl UncleanRun {
    /// Operator-facing description of the vanished run.
    ///
    /// `now_unix` is the current run's start, so the age is expressed relative
    /// to something the operator just witnessed. The raw Unix timestamp is kept
    /// alongside it because that is what correlates the report with the log.
    #[must_use]
    pub fn notice(&self, now_unix: u64) -> String {
        let age = format_age(now_unix.saturating_sub(self.last_seen_unix));
        let pid = self.pid;
        let last_seen = self.last_seen_unix;
        let during = match self.breadcrumb.as_deref() {
            Some(operation) => format!(", during \"{operation}\""),
            None => String::new(),
        };
        format!(
            "Previous jefe run (pid {pid}) ended without a recorded reason; \
             last seen {age} before this start (unix {last_seen}){during}."
        )
    }
}

/// What the current run should do about a marker left behind by another run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorRunDisposition {
    /// Another jefe is running and owns this marker. Leave it alone.
    Concurrent,
    /// The owner is gone and never recorded a reason. Report it and consume it.
    Unclean(UncleanRun),
    /// Liveness is unknown. Report nothing and leave the marker for a later run
    /// that can decide.
    Indeterminate,
}

/// Decide what a marker left by a prior run means for the current run.
#[must_use]
pub fn classify_prior_run(marker: &RunMarker, probe: PriorRunProbe) -> PriorRunDisposition {
    match probe {
        PriorRunProbe::OwnerAlive => PriorRunDisposition::Concurrent,
        PriorRunProbe::Indeterminate => PriorRunDisposition::Indeterminate,
        PriorRunProbe::OwnerGone => PriorRunDisposition::Unclean(UncleanRun {
            pid: marker.identity.pid,
            version: marker.version.clone(),
            last_seen_unix: marker.last_seen_unix,
            breadcrumb: marker.breadcrumb.clone(),
        }),
    }
}

/// Render an elapsed span compactly, without pulling in a date library.
fn format_age(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;

    if seconds < MINUTE {
        format!("{seconds}s")
    } else if seconds < HOUR {
        format!("{}m{:02}s", seconds / MINUTE, seconds % MINUTE)
    } else {
        format!("{}h{:02}m", seconds / HOUR, (seconds % HOUR) / MINUTE)
    }
}
