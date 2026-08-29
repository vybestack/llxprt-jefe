use std::fs;
use std::path::{Path, PathBuf};

use jefe::domain::sha256::Sha256;
use serde_json::Value;

const REQUIRED_SCENARIO_PAIRS: [(&str, &str); 5] = [
    ("provider-health-linux.json", "provider-health-macos.json"),
    (
        "tree-structured-diff-linux.json",
        "tree-structured-diff-macos.json",
    ),
    (
        "workbench-runtime-linux.json",
        "workbench-runtime-macos.json",
    ),
    ("dashboard-parity-linux.json", "dashboard-parity-macos.json"),
    (
        "semantic-continuation-linux.json",
        "semantic-continuation-macos.json",
    ),
];

pub fn validate_scenario_pairs() -> Vec<String> {
    let mut errors = Vec::new();
    let manifest: Value = serde_json::from_str(&read_repo(
        "dev-docs/testing/scenario-execution-manifest.json",
    ))
    .unwrap_or_else(|error| panic!("parse scenario execution manifest: {error}"));
    let owners: Value =
        serde_json::from_str(&read_repo("dev-docs/testing/scenario-owner-evidence.json"))
            .unwrap_or_else(|error| panic!("parse scenario owner evidence: {error}"));
    let manifest_entries = manifest["scenarios"]
        .as_array()
        .unwrap_or_else(|| panic!("scenario execution manifest scenarios must be an array"));
    let owner_entries = owners["scenarios"]
        .as_array()
        .unwrap_or_else(|| panic!("scenario owner evidence scenarios must be an array"));

    for (linux_name, macos_name) in REQUIRED_SCENARIO_PAIRS {
        validate_scenario_pair(
            linux_name,
            macos_name,
            manifest_entries,
            owner_entries,
            &mut errors,
        );
    }
    errors
}

fn validate_scenario_pair(
    linux_name: &str,
    macos_name: &str,
    manifest_entries: &[Value],
    owner_entries: &[Value],
    errors: &mut Vec<String>,
) {
    let pair = [
        format!("dev-docs/tmux-scenarios/issue705/{linux_name}"),
        format!("dev-docs/tmux-scenarios/issue705/{macos_name}"),
    ];
    let entries = pair.each_ref().map(|path| {
        manifest_entries
            .iter()
            .find(|entry| entry["path"].as_str() == Some(path.as_str()))
    });
    let [Some(linux), Some(macos)] = entries else {
        errors.push(format!("scenario pair is incomplete: {pair:?}"));
        return;
    };
    for (entry, required, opposite) in [(linux, "linux", "macos"), (macos, "macos", "linux")] {
        if entry["platforms"][required]["disposition"].as_str() != Some("required")
            || entry["platforms"][opposite]["disposition"].as_str() != Some("unsupported")
        {
            errors.push(format!(
                "scenario pair has invalid native platform ownership: {pair:?}"
            ));
        }
    }
    for field in ["steps_total", "assertions"] {
        if linux["expect"][field] != macos["expect"][field] {
            errors.push(format!("scenario pair {field} differs: {pair:?}"));
        }
    }
    if paired_scenario_body(&pair[0], "linux", errors)
        != paired_scenario_body(&pair[1], "macos", errors)
    {
        errors.push(format!(
            "scenario pair differs beyond name and platform: {pair:?}"
        ));
    }
    validate_scenario_owner_hashes(&pair, owner_entries, errors);
}

fn paired_scenario_body(path: &str, platform: &str, errors: &mut Vec<String>) -> Value {
    let mut scenario: Value = serde_json::from_str(&read_repo(path))
        .unwrap_or_else(|error| panic!("parse {path}: {error}"));
    if scenario["platform"].as_str() != Some(platform) {
        errors.push(format!("scenario declares the wrong platform: {path}"));
    }
    let object = scenario
        .as_object_mut()
        .unwrap_or_else(|| panic!("paired scenario must be an object"));
    object.remove("name");
    object.remove("platform");
    scenario
}

fn validate_scenario_owner_hashes(
    pair: &[String; 2],
    owner_entries: &[Value],
    errors: &mut Vec<String>,
) {
    for path in pair {
        let owner = owner_entries
            .iter()
            .find(|entry| entry["path"].as_str() == Some(path));
        let Some(owner) = owner else {
            errors.push(format!("scenario owner evidence omits {path}"));
            continue;
        };
        match fs::read(repo_path(path)) {
            Ok(bytes) if owner["sha256"].as_str() == Some(&hex_sha256(&bytes)) => {}
            Ok(_) => errors.push(format!("scenario owner hash differs: {path}")),
            Err(error) => errors.push(format!("scenario cannot be read: {path}: {error}")),
        }
    }
}

pub fn semantic_continuation_pair_is_required() -> bool {
    REQUIRED_SCENARIO_PAIRS.contains(&(
        "semantic-continuation-linux.json",
        "semantic-continuation-macos.json",
    ))
}

pub fn one_sided_semantic_continuation_drift_is_detected() -> bool {
    let mut errors = Vec::new();
    let linux = paired_scenario_body(
        "dev-docs/tmux-scenarios/issue705/semantic-continuation-linux.json",
        "linux",
        &mut errors,
    );
    let mut macos = paired_scenario_body(
        "dev-docs/tmux-scenarios/issue705/semantic-continuation-macos.json",
        "macos",
        &mut errors,
    );
    macos["steps"][0]["input"]["key"] = Value::String("drifted".to_owned());
    errors.is_empty() && linux != macos
}

fn read_repo(path: &str) -> String {
    fs::read_to_string(repo_path(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut canonical = Vec::with_capacity(bytes.len());
    let mut offset = 0;
    while let Some(position) = bytes[offset..].windows(2).position(|pair| pair == b"\r\n") {
        let newline = offset + position;
        canonical.extend_from_slice(&bytes[offset..newline]);
        canonical.push(b'\n');
        offset = newline + 2;
    }
    canonical.extend_from_slice(&bytes[offset..]);
    Sha256::digest(&canonical).to_string()
}
