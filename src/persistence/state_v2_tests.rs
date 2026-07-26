//! Strict schema-2 state wire-format tests for CW-01.

use super::diagnostic::CfgCode;
use super::state_v2::StateDocument;

fn minimal_state_json() -> &'static [u8] {
    br#"{
  "state_schema": 2,
  "revision": 7,
  "repositories": [
    {
      "id": "repo.alpha",
      "location": { "local_path": "/tmp/alpha" },
      "display_name": "Alpha",
      "agent_defaults": { "type_id": "core.llxprt", "values": {} }
    }
  ],
  "agents": [
    {
      "id": "agent.alpha",
      "repository_id": "repo.alpha",
      "type_id": "core.llxprt",
      "values": {},
      "launch_signature": {
        "version": 1,
        "definition_hash": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "typed_value_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "target_fingerprint": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
      },
      "runtime": {
        "session_id": "jefe-agent-alpha",
        "invocation_generation": 3,
        "last_known": "unknown"
      }
    }
  ],
  "selection": {
    "repository_id": "repo.alpha",
    "agent_id": "agent.alpha",
    "screen_id": "core.dashboard"
  },
  "last_selected_agent_by_repo": { "repo.alpha": "agent.alpha" },
  "preferences": {
    "hide_idle_repositories": false,
    "pane_focus": "agents",
    "terminal_focused": false,
    "repository_preferences": { "repo.alpha": {} }
  },
  "dormant_records": []
}"#
}

#[test]
fn parses_exact_schema_two_state_and_round_trips_semantically() {
    let Ok(document) = StateDocument::parse(minimal_state_json()) else {
        panic!("valid state-v2 fixture must parse");
    };
    assert_eq!(document.state().revision, 7);
    assert_eq!(document.state().repositories.len(), 1);
    assert_eq!(document.state().agents.len(), 1);
    let Ok(encoded) = document.to_canonical_json() else {
        panic!("valid state-v2 must serialize");
    };
    let Ok(reparsed) = StateDocument::parse(&encoded) else {
        panic!("canonical state-v2 must reparse");
    };
    assert_eq!(reparsed.state(), document.state());
}

#[test]
fn rejects_duplicate_and_unknown_fields_as_malformed_state() {
    let duplicate = br#"{
      "state_schema":2,"state_schema":2,"revision":0,"repositories":[],"agents":[],
      "selection":{},"last_selected_agent_by_repo":{},
      "preferences":{"hide_idle_repositories":false,"pane_focus":"","terminal_focused":false,"repository_preferences":{}},
      "dormant_records":[]
    }"#;
    let unknown = br#"{
      "state_schema":2,"revision":0,"repositories":[],"agents":[],"selection":{},
      "last_selected_agent_by_repo":{},
      "preferences":{"hide_idle_repositories":false,"pane_focus":"","terminal_focused":false,"repository_preferences":{}},
      "dormant_records":[],"unexpected":true
    }"#;
    for source in [duplicate.as_slice(), unknown.as_slice()] {
        let diagnostics = StateDocument::parse(source)
            .err()
            .unwrap_or_else(|| panic!("strict state parser must reject malformed object"));
        assert_eq!(diagnostics[0].code, CfgCode::E103);
    }
}

#[test]
fn rejects_duplicate_ids_and_broken_references() {
    let source = String::from_utf8_lossy(minimal_state_json())
        .replace(
            "\"agent.alpha\",\n      \"repository_id\"",
            "\"repo.alpha\",\n      \"repository_id\"",
        )
        .replace(
            "\"agent_id\": \"agent.alpha\"",
            "\"agent_id\": \"agent.missing\"",
        );
    let diagnostics = StateDocument::parse(source.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("duplicate IDs and broken references must fail"));
    assert!(diagnostics.iter().all(|item| item.code == CfgCode::E006));
    assert!(diagnostics.len() >= 2);
}

#[test]
fn rejects_wrong_schema_and_exactly_one_location_violation() {
    let wrong_schema = String::from_utf8_lossy(minimal_state_json())
        .replace("\"state_schema\": 2", "\"state_schema\": 3");
    let diagnostics = StateDocument::parse(wrong_schema.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("unsupported state schema must fail"));
    assert_eq!(diagnostics[0].code, CfgCode::E103);

    let invalid_location = String::from_utf8_lossy(minimal_state_json()).replace(
        "{ \"local_path\": \"/tmp/alpha\" }",
        "{ \"local_path\": \"/tmp/alpha\", \"remote_target\": \"host:/alpha\" }",
    );
    let diagnostics = StateDocument::parse(invalid_location.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("location must contain exactly one supported field"));
    assert_eq!(diagnostics[0].code, CfgCode::E103);
}

#[test]
fn enforces_state_file_string_array_map_and_depth_bounds() {
    let oversized_file = vec![b' '; super::diagnostic::FILE_LIMIT + 1];
    assert_eq!(
        StateDocument::parse(&oversized_file)
            .err()
            .and_then(|items| items.first().map(|item| item.code)),
        Some(CfgCode::E008)
    );

    let oversized_string = "x".repeat(super::diagnostic::STRING_LIMIT + 1);
    let source = format!(
        "{{\"state_schema\":2,\"revision\":0,\"repositories\":[],\"agents\":[],\"selection\":{{}},\"last_selected_agent_by_repo\":{{}},\"preferences\":{{\"hide_idle_repositories\":false,\"pane_focus\":\"{oversized_string}\",\"terminal_focused\":false,\"repository_preferences\":{{}}}},\"dormant_records\":[]}}"
    );
    assert_eq!(
        StateDocument::parse(source.as_bytes())
            .err()
            .and_then(|items| items.first().map(|item| item.code)),
        Some(CfgCode::E008)
    );
}
