//! Dynamic fixture gates for issue #382 S3a/S3b.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jefe::domain::agent_definition::normalize::Normalize;
use jefe::domain::agent_definition::probe::{
    AnchoredPattern, IdentityRecognizer, evaluate_identity,
};
use jefe::domain::agent_definition::{AgentDefinition, FieldKind};
use serde_json::Value;

use super::fixtures::repo_path;

const FIXTURE_ROOT: &str = "tests/fixtures/agent-definitions";

pub fn assert_all_retained_probe_fixtures() {
    let definitions = AgentDefinition::shipped();
    let fixtures = retained_fixture_dirs();
    assert_eq!(fixtures.len(), 4, "all four retained release directories");
    let mut seen_ids = BTreeSet::new();
    for fixture_dir in fixtures {
        let provenance = read_json(&fixture_dir.join("provenance.json"));
        let type_id = json_str(&provenance, "agent_type_id");
        assert!(seen_ids.insert(type_id.to_string()), "duplicate fixture id");
        let definition = definitions
            .iter()
            .find(|candidate| candidate.id.as_str() == type_id)
            .unwrap_or_else(|| panic!("fixture {type_id} must have a shipped definition"));
        assert_exact_capture_contract(&fixture_dir, &provenance, definition);
        if definition
            .repository_fields
            .iter()
            .any(|field| field.id == "permission_mode")
        {
            assert_claude_permission_modes(&provenance, definition);
        }
    }
    let shipped_ids: BTreeSet<String> = definitions
        .iter()
        .map(|definition| definition.id.as_str().to_string())
        .collect();
    assert_eq!(seen_ids, shipped_ids, "every retained fixture is iterated");
    assert_dynamic_installed_versions(&definitions);
}

fn assert_dynamic_installed_versions(definitions: &[AgentDefinition]) {
    for definition in definitions {
        let stream = dynamic_identity_stream(definition);
        let identity = evaluate_identity(&stream, &definition.probe)
            .unwrap_or_else(|error| panic!("{} dynamic identity: {error}", definition.id));
        assert!(
            identity.is_some(),
            "{} must accept a future installed version without an allow-list",
            definition.id
        );
    }
}

fn dynamic_identity_stream(definition: &AgentDefinition) -> Vec<u8> {
    let IdentityRecognizer::Line {
        prefix,
        anchored_pattern,
    } = &definition.probe.identity
    else {
        panic!("shipped identity probes are text lines");
    };
    let line = match anchored_pattern {
        AnchoredPattern::VersionToken => "17.23.901-nightly.future".to_string(),
        AnchoredPattern::Prefix { prefix: marker } => format!("{marker}17.23.901"),
        AnchoredPattern::Suffix { suffix } => format!("17.23.901 {suffix}"),
        other => panic!("unexpected shipped identity pattern {other:?}"),
    };
    let text = format!("{prefix}{line}\n");
    if definition.probe.normalize == Normalize::StripAnsi {
        format!("\x1b]11;#000000\x07{text}\x1b]104\x07").into_bytes()
    } else {
        text.into_bytes()
    }
}

pub(super) fn retained_fixture_dirs() -> Vec<PathBuf> {
    let root = repo_path(FIXTURE_ROOT);
    let mut fixtures = Vec::new();
    for agent in read_dirs(&root) {
        for release in read_dirs(&agent) {
            assert!(
                release.join("provenance.json").is_file(),
                "retained release {} must carry provenance",
                release.display()
            );
            fixtures.push(release);
        }
    }
    fixtures.sort();
    fixtures
}

fn read_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn assert_exact_capture_contract(
    fixture_dir: &Path,
    provenance: &Value,
    definition: &AgentDefinition,
) {
    let version_stdout = read_bytes(&fixture_dir.join("version.stdout"));
    let probe_stdout = read_bytes(&fixture_dir.join("probe.stdout"));
    assert_eq!(
        probe_stdout, version_stdout,
        "probe and version bytes differ"
    );
    assert_eq!(read_bytes(&fixture_dir.join("version.stderr")), b"");
    assert_eq!(read_bytes(&fixture_dir.join("probe.stderr")), b"");
    assert_eq!(definition.probe.argv, ["--version"]);
    assert_capture_argv(provenance, "probe", &definition.probe.argv);
    assert_capture_argv(provenance, "version", &definition.probe.argv);

    let parsed = evaluate_identity(&probe_stdout, &definition.probe)
        .unwrap_or_else(|error| panic!("{} identity parse: {error}", definition.id));
    let identity = parsed.unwrap_or_else(|| panic!("{} fixture identity", definition.id));
    let release = json_str(&provenance["release"], "version");
    assert!(
        identity.contains(release),
        "{} identity {identity:?} must contain release token {release:?}",
        definition.id
    );
}

fn assert_claude_permission_modes(provenance: &Value, definition: &AgentDefinition) {
    let expected = [
        "acceptEdits",
        "auto",
        "bypassPermissions",
        "manual",
        "dontAsk",
        "plan",
    ];
    let field = definition
        .repository_fields
        .iter()
        .find(|field| field.id == "permission_mode")
        .unwrap_or_else(|| panic!("Claude permission_mode field"));
    assert_eq!(field.kind, FieldKind::Enum);
    assert_eq!(
        field.choices,
        expected.map(str::to_string),
        "only captured and officially verified modes are authored"
    );
    let verified: BTreeSet<&str> = provenance["permission_modes"]
        .as_array()
        .unwrap_or_else(|| panic!("Claude permission_modes"))
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for choice in &field.choices {
        assert!(
            verified.contains(choice.as_str()),
            "{choice} officially verified"
        );
    }
    assert!(
        provenance["official_reference"]["url"].is_string(),
        "Claude modes require an official reference"
    );
}

fn assert_capture_argv(provenance: &Value, capture: &str, expected_tail: &[String]) {
    let argv = provenance["captures"][capture]["argv"]
        .as_array()
        .unwrap_or_else(|| panic!("{capture} argv"));
    let tail: Vec<&str> = argv.iter().skip(1).filter_map(Value::as_str).collect();
    let expected: Vec<&str> = expected_tail.iter().map(String::as_str).collect();
    assert_eq!(tail, expected, "{capture} argv must match definition");
    assert_eq!(provenance["captures"][capture]["exit_code"], 0);
}

pub(super) fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&read_bytes(path))
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

pub(super) fn read_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn json_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string"))
}
