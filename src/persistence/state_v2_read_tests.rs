//! Durable schema-2 read boundary behaviour (issue #381).
//!
//! Startup restores through one reader: schema-2 documents load directly and
//! schema-1 documents are migrated in memory, so a legacy install keeps its
//! repositories, agents and preferences without any on-disk rewrite.

use std::path::Path;

use crate::persistence::{FilePersistenceManager, PersistencePaths};

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

fn manager(root: &Path) -> FilePersistenceManager {
    FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: root.join("settings.toml"),
        state_path: root.join("state.json"),
    })
}

fn schema1_document(work_dir: &Path) -> String {
    let dir = work_dir.display();
    format!(
        r#"{{
  "schema_version": 1,
  "repositories": [
    {{
      "id": "repo-1",
      "name": "Example",
      "slug": "example",
      "base_dir": "{dir}",
      "agent_ids": []
    }}
  ],
  "agents": [
    {{
      "id": "agent-1",
      "display_id": "a1",
      "repository_id": "repo-1",
      "name": "worker",
      "work_dir": "{dir}"
    }}
  ],
  "selected_repository_index": 0,
  "selected_agent_index": 0,
  "hide_idle_repositories": true,
  "last_selected_agent_by_repo": [["repo-1", "agent-1"]],
  "pane_focus": "agents",
  "terminal_focused": false
}}"#
    )
}

/// A missing state file restores an empty document rather than failing, so a
/// first run starts with defaults.
#[test]
fn absent_state_restores_an_empty_durable_document() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let manager = manager(temp.path());

    let restored = manager
        .load_durable_state()
        .value_or_panic("absent state should restore defaults");

    assert!(restored.repositories.is_empty());
    assert!(restored.agents.is_empty());
    assert!(!restored.hide_idle_repositories);
}

/// A schema-1 document on disk is migrated in memory and restored into runtime
/// state, preserving the user's repositories, agents and preferences.
#[test]
fn schema1_state_is_migrated_on_read() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let work_dir = temp.path().join("repo");
    std::fs::create_dir_all(&work_dir).value_or_panic("repository directory");
    std::fs::write(temp.path().join("state.json"), schema1_document(&work_dir))
        .value_or_panic("write schema-1 document");

    let restored = manager(temp.path())
        .load_durable_state()
        .value_or_panic("schema-1 state should migrate on read");

    assert_eq!(restored.repositories.len(), 1);
    assert_eq!(restored.repositories[0].name, "Example");
    assert_eq!(restored.agents.len(), 1);
    assert_eq!(restored.agents[0].name, "worker");
    assert!(restored.hide_idle_repositories);
    assert_eq!(restored.pane_focus, crate::state::PaneFocus::Agents);
    assert_eq!(restored.selected_repository_index, Some(0));
}

/// An unset optional flag is written as an explicit null by the app itself, so
/// migration must accept it. Rejecting the state file leaves the user unable to
/// start at all, which is why this is checked against the real serialized shape
/// rather than a hand-written fixture that omits the field.
#[test]
fn schema1_null_optional_flags_are_migrated() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let work_dir = temp.path().join("repo");
    std::fs::create_dir_all(&work_dir).value_or_panic("repository directory");
    let dir = work_dir.display();
    let document = format!(
        r#"{{
  "schema_version": 1,
  "repositories": [
    {{
      "id": "repo-1",
      "name": "Example",
      "slug": "example",
      "base_dir": "{dir}",
      "default_code_puppy_yolo": null,
      "default_llxprt_version": null,
      "agent_ids": []
    }}
  ],
  "agents": [
    {{
      "id": "agent-1",
      "display_id": "a1",
      "repository_id": "repo-1",
      "name": "worker",
      "work_dir": "{dir}",
      "code_puppy_yolo": null,
      "llxprt_version": null
    }}
  ],
  "selected_repository_index": 0,
  "selected_agent_index": 0,
  "hide_idle_repositories": false,
  "last_selected_agent_by_repo": [],
  "pane_focus": "agents",
  "terminal_focused": false
}}"#
    );
    std::fs::write(temp.path().join("state.json"), document)
        .value_or_panic("write schema-1 document");

    let restored = manager(temp.path())
        .load_durable_state()
        .value_or_panic("null optional flags should migrate");

    assert_eq!(restored.agents.len(), 1);
    assert_eq!(restored.repositories.len(), 1);
}

/// Reading never rewrites the file: migration is a pure in-memory read, so the
/// legacy bytes stay untouched until a save makes a schema-2 document
/// authoritative.
#[test]
fn reading_a_schema1_document_does_not_rewrite_it() {
    let temp = tempfile::tempdir().value_or_panic("temporary state root");
    let work_dir = temp.path().join("repo");
    std::fs::create_dir_all(&work_dir).value_or_panic("repository directory");
    let path = temp.path().join("state.json");
    let original = schema1_document(&work_dir);
    std::fs::write(&path, &original).value_or_panic("write schema-1 document");

    let _ = manager(temp.path())
        .load_durable_state()
        .value_or_panic("schema-1 state should migrate on read");

    let after = std::fs::read_to_string(&path).value_or_panic("re-read state file");
    assert_eq!(after, original, "reading must not rewrite the document");
}
