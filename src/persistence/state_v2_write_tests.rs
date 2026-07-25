//! Durable schema-2 write boundary behaviour (issue #381).
//!
//! Covers the writer path that makes a projected candidate authoritative:
//! canonical bytes, revision fencing, and the one-time schema-1 backup taken
//! when the durable authority is first replaced.

use std::path::{Path, PathBuf};

use crate::domain::{Preferences, Selection, StateV2};
use crate::persistence::writer::Freshness;
use crate::persistence::{FilePersistenceManager, PersistencePaths};
use crate::services::persist_worker::PersistResult;

trait TestResultExt<T, E> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn candidate(revision: u64, hide_idle: bool) -> StateV2 {
    StateV2 {
        state_schema: 2,
        revision,
        repositories: Vec::new(),
        agents: Vec::new(),
        selection: Selection {
            repository_id: None,
            agent_id: None,
            screen_id: None,
        },
        last_selected_agent_by_repo: std::collections::BTreeMap::new(),
        preferences: Preferences {
            hide_idle_repositories: hide_idle,
            pane_focus: "repositories".to_owned(),
            terminal_focused: false,
            repository_preferences: std::collections::BTreeMap::new(),
        },
        dormant_records: Vec::new(),
    }
}

fn manager(temp: &Path) -> FilePersistenceManager {
    FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path: temp.join("state.json"),
    })
}

fn always_current() -> Box<crate::services::persist_worker::FreshnessFn> {
    Box::new(|_| Freshness::Current)
}

fn schema1_bytes() -> Vec<u8> {
    br#"{"schema_version":1,"repositories":[],"agents":[],"selected_repository_index":null,"selected_agent_index":null,"hide_idle_repositories":false,"last_selected_agent_by_repo":[],"pane_focus":"repositories","terminal_focused":false,"user_preferences":{"by_repo":[]}}"#.to_vec()
}

fn backup_files(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .value_or_panic("read state directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".schema1.") && name.contains(".bak"))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn saving_a_candidate_writes_canonical_schema_two_bytes() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let manager = manager(temp.path());

    let result = manager
        .save_state_v2_revisioned(&candidate(1, false), 1, &*always_current())
        .value_or_panic("save schema-2 candidate");

    assert_eq!(result, PersistResult::Authoritative);

    let written =
        std::fs::read(temp.path().join("state.json")).value_or_panic("read durable state");
    let expected = {
        let mut bytes =
            serde_json::to_vec_pretty(&candidate(1, false)).value_or_panic("encode candidate");
        bytes.push(b'\n');
        bytes
    };
    assert_eq!(
        written, expected,
        "durable bytes must match the canonical encoding used by migration"
    );
}

#[test]
fn saved_candidate_reloads_through_the_schema_two_reader() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let manager = manager(temp.path());

    let _ = manager
        .save_state_v2_revisioned(&candidate(4, true), 4, &*always_current())
        .value_or_panic("save schema-2 candidate");

    let bytes = std::fs::read(temp.path().join("state.json")).value_or_panic("read durable state");
    let migration =
        crate::persistence::migration::migrate_state(&bytes).value_or_panic("reload durable state");

    assert!(
        !migration.was_migrated(),
        "schema-2 bytes must be read without migration"
    );
    assert_eq!(migration.state().revision, 4);
    assert!(migration.state().preferences.hide_idle_repositories);
}

#[test]
fn a_stale_candidate_is_not_made_authoritative() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let manager = manager(temp.path());

    let _ = manager
        .save_state_v2_revisioned(&candidate(1, false), 1, &*always_current())
        .value_or_panic("save first candidate");

    let stale: Box<crate::services::persist_worker::FreshnessFn> = Box::new(|_| Freshness::Stale);
    let result = manager
        .save_state_v2_revisioned(&candidate(2, true), 2, &*stale)
        .value_or_panic("attempt stale candidate");

    assert_eq!(result, PersistResult::Stale);

    let bytes = std::fs::read(temp.path().join("state.json")).value_or_panic("read durable state");
    let migration =
        crate::persistence::migration::migrate_state(&bytes).value_or_panic("reload durable");
    assert_eq!(
        migration.state().revision,
        1,
        "the superseded candidate must not replace the authority"
    );
}

#[test]
fn replacing_a_schema_one_authority_retains_a_backup() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let state_path = temp.path().join("state.json");
    let legacy = schema1_bytes();
    std::fs::write(&state_path, &legacy).value_or_panic("seed schema-1 authority");

    let _ = manager(temp.path())
        .save_state_v2_revisioned(&candidate(2, false), 2, &*always_current())
        .value_or_panic("replace schema-1 authority");

    let backups = backup_files(temp.path());
    assert_eq!(
        backups.len(),
        1,
        "replacing a schema-1 authority must retain exactly one backup"
    );
    let retained = std::fs::read(&backups[0]).value_or_panic("read retained backup");
    assert_eq!(
        retained, legacy,
        "the retained backup must be the original schema-1 bytes"
    );
}

#[test]
fn replacing_a_schema_two_authority_takes_no_backup() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let manager = manager(temp.path());

    let _ = manager
        .save_state_v2_revisioned(&candidate(1, false), 1, &*always_current())
        .value_or_panic("save first candidate");
    let _ = manager
        .save_state_v2_revisioned(&candidate(2, true), 2, &*always_current())
        .value_or_panic("save second candidate");

    assert!(
        backup_files(temp.path()).is_empty(),
        "schema-2 replacement must not accumulate backups"
    );
}

#[test]
fn repeated_schema_one_replacement_keeps_one_backup() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let state_path = temp.path().join("state.json");
    std::fs::write(&state_path, schema1_bytes()).value_or_panic("seed schema-1 authority");
    let manager = manager(temp.path());

    let _ = manager
        .save_state_v2_revisioned(&candidate(2, false), 2, &*always_current())
        .value_or_panic("first replacement");
    std::fs::write(&state_path, schema1_bytes()).value_or_panic("reseed schema-1 authority");
    let _ = manager
        .save_state_v2_revisioned(&candidate(3, false), 3, &*always_current())
        .value_or_panic("second replacement");

    assert_eq!(
        backup_files(temp.path()).len(),
        1,
        "identical schema-1 bytes must reuse the content-addressed backup"
    );
}
