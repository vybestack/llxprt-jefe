//! Run-boundary diagnostics (issue #662).
//!
//! A jefe run that dies without warning leaves nothing behind unless it wrote
//! something down while it was still alive. This module is that writing-down:
//!
//! - a `run-start` log record naming the pid, version and start time;
//! - a marker file on disk, refreshed as the run works, naming the operation
//!   in flight;
//! - a `run-end` log record naming a typed reason, after which the marker is
//!   retired.
//!
//! The asymmetry is deliberate. A run that ends for a reason removes its own
//! marker; a run that is killed cannot, so its marker survives it. The next
//! run reads whatever markers are left, asks the operating system whether the
//! recorded owner is still alive, and reports the ones that simply stopped.
//!
//! This module owns the process-global state for the current run, so it is a
//! boundary module: classification itself is pure and lives in
//! [`crate::domain`], and file placement lives in
//! [`crate::persistence::run_marker`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{
    PriorRunDisposition, PriorRunProbe, ProcessIdentity, RunEndReason, RunMarker, UncleanRun,
    classify_prior_run,
};
use crate::persistence::run_marker;
use crate::runtime::{ProcessLiveness, capture_process_identity, process_liveness};

/// Tracing target for every run-boundary record, so a reader can isolate run
/// boundaries in a large log with a single filter.
const LOG_TARGET: &str = "jefe::run";

/// What is recorded when a run had no breadcrumb to offer.
const NO_BREADCRUMB: &str = "none";

struct ActiveRun {
    dir: PathBuf,
    marker: RunMarker,
}

static ACTIVE: Mutex<Option<ActiveRun>> = Mutex::new(None);
static LAST_HEARTBEAT_UNIX: AtomicU64 = AtomicU64::new(0);

/// Holds the current run open.
///
/// Dropping the guard ends the run. [`RunGuard::finish`] states the reason
/// explicitly; an unwinding drop is recorded as [`RunEndReason::Panic`], and
/// any other unannounced drop as [`RunEndReason::Unknown`], so a run that
/// leaves through an unexpected path is still attributed rather than silently
/// abandoned.
///
/// A run that is killed outright never runs this destructor at all. That is
/// the case the surviving marker exists to describe.
pub struct RunGuard {
    reason: Option<RunEndReason>,
}

impl RunGuard {
    /// End the run with an explicit reason.
    pub fn finish(mut self, reason: RunEndReason) {
        self.reason = Some(reason);
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        let reason = match self.reason {
            Some(reason) => reason,
            None if std::thread::panicking() => RunEndReason::Panic,
            None => RunEndReason::Unknown,
        };
        end_run(reason);
    }
}

/// Open a run against `marker_dir`, reporting any prior run that never ended.
///
/// Prior markers are swept before this run writes its own, so a run never
/// reports itself. Each reported run's marker is retired as it is reported, so
/// a single unclean shutdown is surfaced once rather than on every subsequent
/// start. A marker whose owner is still alive belongs to a concurrent jefe and
/// is left untouched; a marker whose owner cannot be probed is left in place
/// for a later start that can answer the question.
#[must_use]
pub fn begin_run(marker_dir: &Path) -> (RunGuard, Vec<UncleanRun>) {
    let unclean = sweep_prior_runs(marker_dir);

    let pid = std::process::id();
    let now = now_unix();
    LAST_HEARTBEAT_UNIX.store(now, Ordering::Relaxed);
    let identity = capture_process_identity(pid).unwrap_or(ProcessIdentity {
        pid,
        started_at: None,
    });
    let marker = RunMarker {
        identity,
        version: crate::VERSION.to_string(),
        started_unix: now,
        last_seen_unix: now,
        breadcrumb: None,
    };

    tracing::info!(
        target: LOG_TARGET,
        event = "run-start",
        pid,
        version = crate::VERSION,
        started_unix = now,
        "jefe run started"
    );

    let _ = run_marker::write_marker(marker_dir, &marker);

    if let Ok(mut active) = ACTIVE.lock() {
        *active = Some(ActiveRun {
            dir: marker_dir.to_path_buf(),
            marker,
        });
    }

    crate::logging::flush();

    (RunGuard { reason: None }, unclean)
}

/// Record the operation the run is currently performing.
///
/// The breadcrumb is durable: it is written into the marker, so a run that is
/// killed mid-operation still names what it was doing. Callers should name
/// coarse, long-lived operations rather than every step, because each call
/// rewrites the marker file.
pub fn record_breadcrumb(operation: &str) {
    refresh(Some(operation));
}

/// Record that the run is still alive.
///
/// Without this the marker's `last_seen` would only ever be its start time,
/// and an unclean shutdown could not say when the run was last known good.
/// A heartbeat skips a refresh already in flight rather than queueing behind it;
/// later ticks remain sufficient, while run retirement and breadcrumbs cannot be
/// starved by redundant marker rewrites.
pub fn heartbeat() {
    let now = now_unix();
    if LAST_HEARTBEAT_UNIX.fetch_max(now, Ordering::Relaxed) >= now {
        return;
    }
    let Ok(mut active) = ACTIVE.try_lock() else {
        return;
    };
    refresh_active_at(&mut active, None, now);
}

/// Record that the host is tearing this run down, and retire the run.
///
/// Called from the platform's console control handler, which the OS runs on an
/// injected thread with only a few seconds (far less at machine shutdown)
/// before the process is killed outright. No destructor, no unwind, and no
/// exit path runs afterwards, so the record has to be complete and flushed by
/// the time this returns.
///
/// This deliberately does not terminate the process. The events that reach it
/// are already fatal — the OS kills the process once the handler returns — and
/// exiting here would mean stealing `Ctrl-C` from the attached agent terminal
/// in the one case where it is not fatal.
pub fn record_host_termination() {
    end_run(RunEndReason::HostTerminated);
}

/// Ask the OS to tell this run when the console is closing.
///
/// Closing a console window, logging off, and shutting down all kill jefe
/// without running any of its code, which is exactly the anonymous death this
/// module exists to eliminate. Registering a handler converts that into a
/// recorded reason.
///
/// Registration failures are swallowed: a run that cannot install the handler
/// is no worse off than one that never tried, and refusing to start over it
/// would trade a diagnostic for an outage.
pub fn install_host_termination_handler() {
    #[cfg(windows)]
    {
        // `SetConsoleCtrlHandler` takes a raw FFI callback, which this package
        // forbids; ctrlc owns that unsafe block. Its `termination` feature is
        // what widens the registration past Ctrl-C to the close, logoff, and
        // shutdown events that actually kill an unattended run.
        let _ = ctrlc::try_set_handler(record_host_termination);
    }
}

/// Rewrite the marker of the run that is still open.
///
/// The write happens while the run is held, not after a snapshot of it is
/// taken. A refresh that released the lock before writing could be overtaken
/// by [`end_run`] and land *after* the marker was retired, resurrecting it and
/// making the next start report a clean quit as an unclean death. Holding the
/// run across the write also serializes concurrent refreshes, so a heartbeat
/// and a breadcrumb never contend for the same scratch file.
fn refresh(breadcrumb: Option<&str>) {
    let Ok(mut active) = ACTIVE.lock() else {
        return;
    };
    refresh_active(&mut active, breadcrumb);
}

fn refresh_active(active: &mut Option<ActiveRun>, breadcrumb: Option<&str>) {
    refresh_active_at(active, breadcrumb, now_unix());
}

fn refresh_active_at(active: &mut Option<ActiveRun>, breadcrumb: Option<&str>, now: u64) {
    let Some(run) = active.as_mut() else {
        return;
    };

    run.marker.last_seen_unix = now;
    if let Some(operation) = breadcrumb {
        run.marker.breadcrumb = Some(operation.to_string());
    }
    let _ = run_marker::write_marker(&run.dir, &run.marker);
}

fn end_run(reason: RunEndReason) {
    // Retiring the run and removing its marker under one lock is what makes
    // the retirement final: a concurrent refresh either completes before the
    // run is taken, or finds no run and declines to write.
    let taken = {
        let Ok(mut active) = ACTIVE.lock() else {
            return;
        };
        let taken = active.take();
        if let Some(run) = taken.as_ref() {
            run_marker::remove_marker(&run.dir, run.marker.identity.pid);
        }
        taken
    };
    let Some(run) = taken else {
        return;
    };

    tracing::info!(
        target: LOG_TARGET,
        event = "run-end",
        pid = run.marker.identity.pid,
        reason = reason.as_str(),
        breadcrumb = run.marker.breadcrumb.as_deref().unwrap_or(NO_BREADCRUMB),
        "jefe run ended"
    );

    crate::logging::flush();
}

fn sweep_prior_runs(dir: &Path) -> Vec<UncleanRun> {
    let mut unclean = Vec::new();

    for stored in run_marker::read_markers(dir) {
        let probe = probe_owner(&stored.marker);
        match classify_prior_run(&stored.marker, probe) {
            PriorRunDisposition::Unclean(run) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    event = "prior-run-unclean",
                    pid = run.pid,
                    version = run.version.as_str(),
                    last_seen_unix = run.last_seen_unix,
                    breadcrumb = run.breadcrumb.as_deref().unwrap_or(NO_BREADCRUMB),
                    "prior jefe run ended without a recorded reason"
                );
                run_marker::remove_marker(dir, run.pid);
                unclean.push(run);
            }
            PriorRunDisposition::Concurrent | PriorRunDisposition::Indeterminate => {}
        }
    }

    unclean
}

fn probe_owner(marker: &RunMarker) -> PriorRunProbe {
    match process_liveness(Some(marker.identity)) {
        ProcessLiveness::Alive => PriorRunProbe::OwnerAlive,
        ProcessLiveness::Dead | ProcessLiveness::ReusedPid => PriorRunProbe::OwnerGone,
        ProcessLiveness::Inaccessible
        | ProcessLiveness::MalformedIdentity
        | ProcessLiveness::ProbeFailure => PriorRunProbe::Indeterminate,
    }
}

/// Current wall-clock time in Unix seconds, as run records express it.
///
/// A clock that cannot be read yields zero rather than refusing, because a
/// diagnostic that declines to record itself defeats its own purpose.
#[must_use]
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
