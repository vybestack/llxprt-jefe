//! Issue #662: a jefe run must leave an attributable record of its own boundaries.
//!
//! The failure this covers is a process death that produced no artifact at all.
//! The only mechanism that can attribute such a death is state written *before*
//! it and interpreted *after* it, so these tests exercise the run marker, its
//! classification against the recorded owner's liveness, and the operator-facing
//! notice produced when a prior run ended with no recorded reason.

use jefe::domain::{
    PriorRunDisposition, PriorRunProbe, ProcessIdentity, RunEndReason, RunMarker,
    classify_prior_run,
};
use jefe::persistence::run_marker;

fn marker_with(pid: u32, last_seen_unix: u64, breadcrumb: Option<&str>) -> RunMarker {
    RunMarker {
        identity: ProcessIdentity::new(pid, 4_242),
        version: "0.0.32".to_string(),
        started_unix: 1_780_400_000,
        last_seen_unix,
        breadcrumb: breadcrumb.map(str::to_string),
    }
}

#[test]
fn prior_run_whose_owner_is_gone_is_reported_as_unclean() {
    let marker = marker_with(4321, 1_780_412_566, Some("attach agent-3"));

    let disposition = classify_prior_run(&marker, PriorRunProbe::OwnerGone);

    let PriorRunDisposition::Unclean(unclean) = disposition else {
        panic!("a marker whose owner is gone must be reported: {disposition:?}");
    };
    assert_eq!(unclean.pid, 4321);
    assert_eq!(unclean.last_seen_unix, 1_780_412_566);
    assert_eq!(unclean.breadcrumb.as_deref(), Some("attach agent-3"));
}

#[test]
fn prior_run_whose_owner_is_alive_is_a_concurrent_instance() {
    let marker = marker_with(4321, 1_780_412_566, None);

    let disposition = classify_prior_run(&marker, PriorRunProbe::OwnerAlive);

    assert_eq!(disposition, PriorRunDisposition::Concurrent);
}

#[test]
fn prior_run_with_indeterminate_liveness_is_not_reported() {
    let marker = marker_with(4321, 1_780_412_566, None);

    let disposition = classify_prior_run(&marker, PriorRunProbe::Indeterminate);

    assert_eq!(disposition, PriorRunDisposition::Indeterminate);
}

#[test]
fn unclean_notice_names_pid_last_seen_and_breadcrumb() {
    let marker = marker_with(4321, 1_780_412_566, Some("attach agent-3"));
    let PriorRunDisposition::Unclean(unclean) =
        classify_prior_run(&marker, PriorRunProbe::OwnerGone)
    else {
        panic!("expected an unclean prior run");
    };

    let notice = unclean.notice(1_780_412_703);

    assert!(
        notice.contains("4321"),
        "notice must name the pid: {notice}"
    );
    assert!(
        notice.contains("1780412566"),
        "notice must name the last-seen timestamp: {notice}"
    );
    assert!(
        notice.contains("2m17s"),
        "notice must say how long before this start: {notice}"
    );
    assert!(
        notice.contains("attach agent-3"),
        "notice must name the in-flight operation: {notice}"
    );
    assert!(
        notice.contains("without a recorded reason"),
        "notice must state the run ended unattributed: {notice}"
    );
}

#[test]
fn unclean_notice_omits_breadcrumb_when_none_was_recorded() {
    let marker = marker_with(4321, 1_780_412_566, None);
    let PriorRunDisposition::Unclean(unclean) =
        classify_prior_run(&marker, PriorRunProbe::OwnerGone)
    else {
        panic!("expected an unclean prior run");
    };

    let notice = unclean.notice(1_780_412_570);

    assert!(
        notice.contains("4321"),
        "notice must name the pid: {notice}"
    );
    assert!(
        !notice.contains("during"),
        "notice must not invent an operation it never recorded: {notice}"
    );
}

#[test]
fn last_seen_after_the_current_clock_reports_a_zero_age_rather_than_underflowing() {
    let marker = marker_with(4321, 1_780_412_600, None);
    let PriorRunDisposition::Unclean(unclean) =
        classify_prior_run(&marker, PriorRunProbe::OwnerGone)
    else {
        panic!("expected an unclean prior run");
    };

    let notice = unclean.notice(1_780_412_566);

    assert!(notice.contains("0s"), "clock skew must not panic: {notice}");
}

#[test]
fn run_end_reasons_have_stable_log_labels() {
    assert_eq!(RunEndReason::UserQuit.as_str(), "user-quit");
    assert_eq!(RunEndReason::RenderFailed.as_str(), "render-failed");
    assert_eq!(RunEndReason::Panic.as_str(), "panic");
    assert_eq!(RunEndReason::HostTerminated.as_str(), "host-terminated");
    assert_eq!(RunEndReason::Unknown.as_str(), "unknown");
}

#[test]
fn run_marker_round_trips_and_tolerates_unknown_fields() {
    let marker = marker_with(4321, 1_780_412_566, Some("detach agent-1"));

    let Ok(encoded) = serde_json::to_string(&marker) else {
        panic!("a run marker must serialize");
    };
    let Ok(decoded) = serde_json::from_str::<RunMarker>(&encoded) else {
        panic!("a run marker must round-trip: {encoded}");
    };
    assert_eq!(decoded, marker);

    let forward = r#"{"identity":{"pid":77,"started_at":9},"version":"9.9.9",
        "started_unix":1,"last_seen_unix":2,"future_field":true}"#;
    let Ok(decoded) = serde_json::from_str::<RunMarker>(forward) else {
        panic!("a marker written by a newer jefe must still be readable");
    };
    assert_eq!(decoded.identity.pid, 77);
    assert_eq!(decoded.breadcrumb, None);
}

// ---------------------------------------------------------------------------
// Marker persistence: the record a live run leaves for a later one to read.
// ---------------------------------------------------------------------------

fn temp_root(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("jefe-issue662-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let Ok(()) = std::fs::create_dir_all(&root) else {
        panic!("could not create the test root");
    };
    root
}

fn write(dir: &std::path::Path, marker: &RunMarker) {
    let Ok(_) = run_marker::write_marker(dir, marker) else {
        panic!("writing a run marker must succeed in a writable directory");
    };
}

#[test]
fn marker_directory_sits_beside_the_durable_state_file() {
    let state = std::path::Path::new("/config/state.json");

    let dir = run_marker::run_marker_dir(state);

    assert_eq!(dir.file_name(), Some(std::ffi::OsStr::new("runs")));
    assert_eq!(dir.parent(), state.parent());
}

#[test]
fn a_written_marker_is_readable_by_a_later_run() {
    let root = temp_root("roundtrip");
    let marker = marker_with(4321, 1_780_412_566, Some("attach agent-3"));

    write(&root, &marker);
    let found = run_marker::read_markers(&root);

    assert_eq!(found.len(), 1, "exactly one marker was written");
    assert_eq!(found[0].marker, marker);
}

#[test]
fn rewriting_a_marker_replaces_it_instead_of_accumulating_files() {
    let root = temp_root("replace");
    write(&root, &marker_with(4321, 1_780_412_566, None));
    write(
        &root,
        &marker_with(4321, 1_780_412_999, Some("detach agent-1")),
    );

    let found = run_marker::read_markers(&root);

    assert_eq!(found.len(), 1, "one run owns one marker");
    assert_eq!(found[0].marker.last_seen_unix, 1_780_412_999);
    assert_eq!(
        found[0].marker.breadcrumb.as_deref(),
        Some("detach agent-1")
    );
}

#[test]
fn concurrent_runs_each_keep_their_own_marker() {
    let root = temp_root("concurrent");
    write(&root, &marker_with(4321, 1_780_412_566, None));
    write(&root, &marker_with(8765, 1_780_412_567, None));

    let mut pids: Vec<u32> = run_marker::read_markers(&root)
        .into_iter()
        .map(|stored| stored.marker.identity.pid)
        .collect();
    pids.sort_unstable();

    assert_eq!(pids, vec![4321, 8765]);
}

#[test]
fn a_missing_marker_directory_reads_as_no_prior_runs() {
    let root = temp_root("missing");
    let absent = root.join("never-created");

    assert!(run_marker::read_markers(&absent).is_empty());
}

#[test]
fn an_unreadable_marker_is_discarded_rather_than_reported_forever() {
    let root = temp_root("corrupt");
    let corrupt = root.join("run-4321.json");
    let Ok(()) = std::fs::write(&corrupt, b"{ not json") else {
        panic!("could not seed a corrupt marker");
    };

    let found = run_marker::read_markers(&root);

    assert!(
        found.is_empty(),
        "a marker that cannot be read says nothing"
    );
    assert!(
        !corrupt.exists(),
        "an uninterpretable marker must not accumulate across every future start"
    );
}

#[test]
fn unrelated_files_in_the_marker_directory_are_left_alone() {
    let root = temp_root("foreign");
    let foreign = root.join("notes.txt");
    let Ok(()) = std::fs::write(&foreign, b"operator notes") else {
        panic!("could not seed an unrelated file");
    };
    write(&root, &marker_with(4321, 1_780_412_566, None));

    let found = run_marker::read_markers(&root);

    assert_eq!(found.len(), 1);
    assert!(
        foreign.exists(),
        "only jefe run markers are jefe's to delete"
    );
}

#[test]
fn removing_a_marker_retires_only_that_run() {
    let root = temp_root("remove");
    write(&root, &marker_with(4321, 1_780_412_566, None));
    write(&root, &marker_with(8765, 1_780_412_567, None));

    run_marker::remove_marker(&root, 4321);

    let found = run_marker::read_markers(&root);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].marker.identity.pid, 8765);
}

// ---------------------------------------------------------------------------
// Run boundaries: what a run writes about itself, and what survives its death.
//
// These cases re-execute this test binary as a child so that a real process
// death can be observed from the outside. The child branch is selected by
// `JEFE_ISSUE662_CHILD`; the parent branch asserts on what the dead child left
// behind.
// ---------------------------------------------------------------------------

const CHILD_MARKER_ENV: &str = "JEFE_ISSUE662_CHILD";
const CHILD_ROOT_ENV: &str = "JEFE_ISSUE662_ROOT";

fn child_root() -> Option<std::path::PathBuf> {
    if std::env::var(CHILD_MARKER_ENV).is_err() {
        return None;
    }
    std::env::var(CHILD_ROOT_ENV)
        .ok()
        .map(std::path::PathBuf::from)
}

fn run_child(test_name: &str, root: &std::path::Path) -> std::process::Output {
    let Ok(exe) = std::env::current_exe() else {
        panic!("the test binary must know its own path");
    };
    let Ok(output) = std::process::Command::new(exe)
        .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
        .env(CHILD_MARKER_ENV, "1")
        .env(CHILD_ROOT_ENV, root)
        .env("JEFE_LOG_FILE", root.join("jefe.log"))
        .env("JEFE_LOG", "info")
        .output()
    else {
        panic!("the child run must be launchable");
    };
    output
}

fn child_log(root: &std::path::Path) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("jefe.log")) else {
        panic!("a run that initialized logging must have produced a log file");
    };
    text
}

#[test]
fn a_run_that_ends_for_a_reason_records_both_boundaries_and_retires_its_marker() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);
        guard.finish(RunEndReason::UserQuit);
        std::process::exit(0);
    }

    let root = temp_root("clean-exit");
    let output = run_child(
        "a_run_that_ends_for_a_reason_records_both_boundaries_and_retires_its_marker",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let log = child_log(&root);
    assert!(log.contains("run-start"), "missing run-start record: {log}");
    assert!(log.contains("run-end"), "missing run-end record: {log}");
    assert!(
        log.contains("user-quit"),
        "run-end must name a typed reason: {log}"
    );
    assert!(
        log.contains(jefe::VERSION),
        "run-start must record the version: {log}"
    );
    assert!(
        run_marker::read_markers(&root).is_empty(),
        "a run that ended for a recorded reason must retire its marker"
    );
}

#[test]
fn the_run_end_record_names_the_last_operation_the_run_was_doing() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);
        jefe::run_diagnostics::record_breadcrumb("attach agent-11");
        guard.finish(RunEndReason::RenderFailed);
        std::process::exit(0);
    }

    let root = temp_root("end-breadcrumb");
    let output = run_child(
        "the_run_end_record_names_the_last_operation_the_run_was_doing",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let log = child_log(&root);
    let ended = log
        .lines()
        .find(|line| line.contains("run-end"))
        .unwrap_or_else(|| panic!("missing run-end record: {log}"));
    assert!(
        ended.contains("attach agent-11"),
        "the run-end record must carry the breadcrumb, because the marker is \
         deleted on a clean exit and the log is then the only place an operator \
         can see what the run was last doing: {ended}"
    );
    assert!(
        ended.contains("render-failed"),
        "the run-end record must still name its typed reason: {ended}"
    );
}

#[test]
fn a_run_killed_without_a_reason_leaves_its_marker_and_its_last_breadcrumb() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);
        jefe::run_diagnostics::record_breadcrumb("attach agent-7");
        // Simulate an external kill: no unwinding, no destructors, no exit path.
        std::mem::forget(guard);
        std::process::exit(0);
    }

    let root = temp_root("killed");
    let output = run_child(
        "a_run_killed_without_a_reason_leaves_its_marker_and_its_last_breadcrumb",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let log = child_log(&root);
    assert!(
        log.contains("run-start"),
        "the start record must survive an abrupt death: {log}"
    );
    assert!(
        !log.contains("run-end"),
        "a killed run must not claim a recorded end: {log}"
    );

    let found = run_marker::read_markers(&root);
    assert_eq!(found.len(), 1, "the killed run must have left its marker");
    assert_eq!(
        found[0].marker.breadcrumb.as_deref(),
        Some("attach agent-7"),
        "the marker must name what the run was doing when it died"
    );
}

#[test]
fn a_panicking_run_still_records_why_it_ended() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (_guard, _prior) = jefe::run_diagnostics::begin_run(&root);
        panic!("simulated unrecoverable failure");
    }

    let root = temp_root("panic-exit");
    let output = run_child("a_panicking_run_still_records_why_it_ended", &root);
    assert!(
        !output.status.success(),
        "the panicking child should have failed"
    );

    let log = child_log(&root);
    assert!(
        log.contains("run-end"),
        "an unwinding run must still record its end: {log}"
    );
    assert!(
        log.contains("panic"),
        "run-end must attribute the panic: {log}"
    );
}

#[test]
fn a_new_run_reports_and_clears_the_marker_of_a_prior_run_that_never_ended() {
    let root = temp_root("prior-unclean");
    let dead = RunMarker {
        identity: ProcessIdentity::new(4_000_000_001, 4_242),
        version: "0.0.31".to_string(),
        started_unix: 1_780_400_000,
        last_seen_unix: 1_780_412_566,
        breadcrumb: Some("attach agent-3".to_string()),
    };
    write(&root, &dead);

    let (guard, prior) = jefe::run_diagnostics::begin_run(&root);

    assert_eq!(prior.len(), 1, "the abandoned run must be reported");
    assert_eq!(prior[0].pid, 4_000_000_001);
    assert_eq!(prior[0].last_seen_unix, 1_780_412_566);
    assert_eq!(prior[0].breadcrumb.as_deref(), Some("attach agent-3"));

    guard.finish(RunEndReason::UserQuit);

    let leftover = run_marker::read_markers(&root);
    assert!(
        leftover.is_empty(),
        "a reported prior run must not be reported again forever"
    );
}

#[test]
fn a_heartbeat_in_flight_at_shutdown_cannot_resurrect_a_retired_marker() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut beating = Vec::new();
        for _ in 0..4 {
            let stop = std::sync::Arc::clone(&stop);
            beating.push(std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    jefe::run_diagnostics::heartbeat();
                }
            }));
        }

        // Let the heartbeats saturate, then end the run underneath them, which
        // is exactly what happens when the render loop exits while a dispatched
        // heartbeat is still on the blocking pool.
        std::thread::sleep(std::time::Duration::from_millis(100));
        guard.finish(RunEndReason::UserQuit);
        std::thread::sleep(std::time::Duration::from_millis(100));

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for thread in beating {
            let _ = thread.join();
        }
        std::process::exit(0);
    }

    let root = temp_root("heartbeat-shutdown-race");
    let output = run_child(
        "a_heartbeat_in_flight_at_shutdown_cannot_resurrect_a_retired_marker",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let log = child_log(&root);
    assert!(
        log.contains("user-quit"),
        "the run must still have recorded a clean end: {log}"
    );
    assert!(
        run_marker::read_markers(&root).is_empty(),
        "a run that ended for a recorded reason must stay retired; a late \
         heartbeat that rewrites the marker makes the next start report a \
         clean quit as an unclean death"
    );
}

#[test]
fn a_breadcrumb_recorded_while_the_run_heartbeats_survives_the_kill() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);

        // The real run records breadcrumbs from the attach worker while the
        // heartbeat future refreshes the same marker, both on the blocking
        // pool. Neither may drop the other's write.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let beating = {
            let stop = std::sync::Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    jefe::run_diagnostics::heartbeat();
                }
            })
        };

        for step in 0..200 {
            jefe::run_diagnostics::record_breadcrumb(&format!("attach agent-{step}"));
        }
        jefe::run_diagnostics::record_breadcrumb("attach agent-final");

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = beating.join();

        // Simulate an external kill: no unwinding, no destructors.
        std::mem::forget(guard);
        std::process::exit(0);
    }

    let root = temp_root("concurrent-breadcrumb");
    let output = run_child(
        "a_breadcrumb_recorded_while_the_run_heartbeats_survives_the_kill",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let found = run_marker::read_markers(&root);
    assert_eq!(found.len(), 1, "the killed run must own exactly one marker");
    assert_eq!(
        found[0].marker.breadcrumb.as_deref(),
        Some("attach agent-final"),
        "a heartbeat running alongside the breadcrumb must not cost the run \
         the last operation it recorded, which is the whole point of the \
         breadcrumb"
    );
}

#[test]
fn a_run_the_host_tears_down_records_why_before_it_dies() {
    if let Some(root) = child_root() {
        jefe::logging::init();
        let (guard, _prior) = jefe::run_diagnostics::begin_run(&root);
        jefe::run_diagnostics::record_breadcrumb("attach agent-13");
        // The console control handler runs on a thread the OS injects, with a
        // few seconds before the process is killed outright. Nothing after this
        // is guaranteed to run, so the death is simulated the same way an
        // external kill is: no unwinding, no destructors, no exit path.
        jefe::run_diagnostics::record_host_termination();
        std::mem::forget(guard);
        std::process::exit(0);
    }

    let root = temp_root("host-teardown");
    let output = run_child(
        "a_run_the_host_tears_down_records_why_before_it_dies",
        &root,
    );
    assert!(output.status.success(), "child run should have exited 0");

    let log = child_log(&root);
    let ended = log
        .lines()
        .find(|line| line.contains("run-end"))
        .unwrap_or_else(|| panic!("a host teardown must still record a run end: {log}"));
    assert!(
        ended.contains("host-terminated"),
        "the run end must name the host teardown as its typed reason, so the \
         death is attributable instead of anonymous: {ended}"
    );
    assert!(
        ended.contains("attach agent-13"),
        "the run end must still name the operation in flight: {ended}"
    );

    assert!(
        run_marker::read_markers(&root).is_empty(),
        "a death the run explained must retire its marker, or the next start \
         would also report it as having ended without a recorded reason"
    );
}
