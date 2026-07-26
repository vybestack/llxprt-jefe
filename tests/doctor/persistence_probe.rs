//! RED contract: read-only persistence probe for `jefe doctor`
//! (issue #264, AC-08 / decision D-05).
//!
//! `probe_persistence` is expected to be a pure-ish function under
//! `jefe::doctor` that, given a candidate config directory, determines whether
//! the directory is usable for persistence *without initializing it*. The
//! contract (decision D-05):
//!
//! - For a *missing* directory, the probe reports `Absent` and must NOT create
//!   the directory or any of its ancestors. Existing `validate_config_dir`
//!   creates a missing directory, so `doctor` must not reuse it directly.
//! - For an *existing writable* directory, the probe reports `Writable` (or a
//!   `Pass` finding) and must clean up any transient writability probe it
//!   created, leaving the directory structurally identical to before the call.
//! - For an existing directory that is not writable, the probe reports a
//!   blocking failure naming the structural path.
//!
//! These tests use real `tempfile::TempDir` directories (no mocks) so the
//! observable filesystem side effects are asserted directly.

use jefe::doctor::{PersistenceProbeOutcome, probe_persistence};

use crate::support::TestResultExt;

#[test]
fn missing_config_directory_is_reported_absent_and_not_created() {
    // A TempDir that we never materialize on disk models a missing config dir.
    let parent = tempfile::tempdir().test_unwrap("create parent tempdir");
    let missing = parent.path().join("does-not-exist");
    assert!(
        !missing.exists(),
        "precondition: missing config dir must not exist yet"
    );

    let outcome = probe_persistence(&missing).test_unwrap("probe missing dir");
    assert_eq!(
        outcome,
        PersistenceProbeOutcome::Absent,
        "a missing config directory must be reported Absent"
    );
    assert!(
        !missing.exists(),
        "probe must NOT create a missing config directory (decision D-05)"
    );
    assert!(
        !parent.path().join("does-not-exist").exists(),
        "probe must not materialize the directory under any spelling"
    );
}

#[test]
fn existing_writable_directory_is_writable_and_leaves_no_probe_artifact() {
    let dir = tempfile::tempdir().test_unwrap("create writable tempdir");
    let config = dir.path().to_path_buf();
    let snapshot_before = list_dir_entries(&config);

    let outcome = probe_persistence(&config).test_unwrap("probe writable dir");
    assert_eq!(
        outcome,
        PersistenceProbeOutcome::Writable,
        "an existing writable directory must be reported Writable"
    );

    let snapshot_after = list_dir_entries(&config);
    assert_eq!(
        snapshot_before, snapshot_after,
        "probe must clean up its transient writability file and leave no artifacts"
    );
}

#[test]
fn existing_writable_directory_cleans_transient_probe_recursively() {
    // Even when the probe writes into a nested existing subdirectory, the
    // post-probe directory tree must be identical to the pre-probe tree.
    let dir = tempfile::tempdir().test_unwrap("create nested tempdir");
    let nested = dir.path().join("state");
    std::fs::create_dir_all(&nested).test_unwrap("create nested state dir");
    let before = list_dir_entries(&nested);

    let outcome = probe_persistence(&nested).test_unwrap("probe nested dir");
    assert_eq!(outcome, PersistenceProbeOutcome::Writable);

    let after = list_dir_entries(&nested);
    assert_eq!(
        before, after,
        "nested probe must clean up its transient writability file"
    );
}

#[test]
fn probe_does_not_touch_settings_or_state_files() {
    // The probe must be read-only with respect to real persistence payloads:
    // existing settings.toml / state.json contents must be byte-identical
    // after the probe runs.
    let dir = tempfile::tempdir().test_unwrap("create populated tempdir");
    let settings_path = dir.path().join("settings.toml");
    let state_path = dir.path().join("state.json");
    std::fs::write(&settings_path, SAMPLE_SETTINGS).test_unwrap("seed settings.toml");
    std::fs::write(&state_path, SAMPLE_STATE).test_unwrap("seed state.json");

    let _ = probe_persistence(dir.path()).test_unwrap("probe populated dir");

    let settings_after =
        std::fs::read_to_string(&settings_path).test_unwrap("read settings.toml after probe");
    let state_after =
        std::fs::read_to_string(&state_path).test_unwrap("read state.json after probe");
    assert_eq!(
        settings_after, SAMPLE_SETTINGS,
        "probe must not modify settings.toml"
    );
    assert_eq!(
        state_after, SAMPLE_STATE,
        "probe must not modify state.json"
    );
}

#[test]
fn probe_existing_directory_is_not_reported_absent() {
    // Sanity guard: an existing writable dir must never be misclassified as
    // Absent (which would imply doctor thinks the user has no config).
    let dir = tempfile::tempdir().test_unwrap("create existing tempdir");
    let outcome = probe_persistence(dir.path()).test_unwrap("probe existing dir");
    assert_ne!(
        outcome,
        PersistenceProbeOutcome::Absent,
        "an existing directory must not be reported Absent"
    );
}

/// Recursively collect the sorted set of relative entry paths under `dir`.
/// Used to prove a probe leaves the directory tree structurally unchanged.
fn list_dir_entries(dir: &std::path::Path) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    collect_relative(dir, dir, &mut entries);
    entries.sort();
    entries
}

fn collect_relative(root: &std::path::Path, current: &std::path::Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(current) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
        if path.is_dir() {
            collect_relative(root, &path, out);
        }
    }
}

const SAMPLE_SETTINGS: &str = "schema_version = 1\ntheme = \"green-screen\"\n";
const SAMPLE_STATE: &str = "{\"schema_version\":1,\"repositories\":[],\"agents\":[],\"selected_repository_index\":null,\"selected_agent_index\":null,\"hide_idle_repositories\":false,\"last_selected_agent_by_repo\":[],\"pane_focus\":\"\",\"terminal_focused\":false,\"user_preferences\":{}}";
