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
