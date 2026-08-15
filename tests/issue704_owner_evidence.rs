//! Immutable acceptance-owner evidence for issue #704 (CWR1-00..CWR1-10).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use jefe::domain::sha256::Sha256;
use serde::Deserialize;
use serde_json::Value;

const EVIDENCE_PATH: &str = "dev-docs/testing/issue704-owner-evidence.json";
const CRITERIA: [&str; 11] = [
    "CWR1-00", "CWR1-01", "CWR1-02", "CWR1-03", "CWR1-04", "CWR1-05", "CWR1-06", "CWR1-07",
    "CWR1-08", "CWR1-09", "CWR1-10",
];
const PLATFORMS: [&str; 3] = ["linux", "macos", "windows"];
const COMMANDS: [&str; 10] = [
    "cargo check --workspace --all-targets --all-features --locked --target x86_64-pc-windows-msvc",
    "cargo test --locked --all-features --test issue390",
    "cargo test --locked --all-features --test issue704",
    "cargo test --locked --all-features --test issue704_owner_evidence",
    "cargo test --locked --all-features --test scenario_manifest",
    "cargo xtask ci",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue704/atomic-success.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue704/provider-crash.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue704/required-provider-failure.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue704/restart-publication.json",
];
const STATIC_FAILURE_COMMAND: &str =
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue704/static-failure.json";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    schema: u32,
    issue: u32,
    artifacts: Vec<Artifact>,
    criteria: BTreeMap<String, Criterion>,
    production_symbols: Vec<SymbolOwner>,
    deleted_symbols: Vec<DeletedSymbol>,
    deleted_paths: Vec<DeletedPath>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: String,
    sha256: String,
    mode: String,
    criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Criterion {
    paths: Vec<String>,
    commands: Vec<CommandOwner>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandOwner {
    command: String,
    platforms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolOwner {
    path: String,
    symbol: String,
    criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedSymbol {
    scope: String,
    symbol: String,
    criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedPath {
    path: String,
    criteria: Vec<String>,
}

#[test]
fn issue704_owner_evidence_is_exact_and_current() {
    assert_valid(&load_value());
}

#[test]
fn stale_artifact_hash_is_rejected() {
    let mut value = load_value();
    value["artifacts"][0]["sha256"] = Value::String("0".repeat(64));
    assert_rejected(&value, "hash differs");
}

#[test]
fn missing_or_duplicate_criteria_are_rejected() {
    let mut missing = load_value();
    remove_object_key(&mut missing["criteria"], "CWR1-10");
    assert_rejected(&missing, "criteria keys differ");

    let mut duplicate = load_value();
    let criterion = duplicate["artifacts"][0]["criteria"][0].clone();
    push_array(&mut duplicate["artifacts"][0]["criteria"], criterion);
    assert_rejected(&duplicate, "criteria must be sorted and unique");
}

#[test]
fn invalid_command_or_platform_is_rejected() {
    let mut command = load_value();
    command["criteria"]["CWR1-00"]["commands"][0]["command"] =
        Value::String("cargo test --ignored".into());
    assert_rejected(&command, "command is not approved");

    let mut platform = load_value();
    platform["criteria"]["CWR1-00"]["commands"][0]["platforms"][0] =
        Value::String("solaris".into());
    assert_rejected(&platform, "platforms must be sorted, unique, and closed");

    let mut unsupported_tmux = load_value();
    unsupported_tmux["criteria"]["CWR1-01"]["commands"][1]["platforms"] =
        serde_json::json!(["macos", "windows"]);
    assert_rejected(
        &unsupported_tmux,
        "tmux command platform differs from its scenario",
    );
}

#[test]
fn stale_symbol_and_resurrected_authority_are_rejected() {
    let mut stale = load_value();
    stale["production_symbols"][0]["symbol"] = Value::String("missing_issue704_symbol".into());
    assert_rejected(&stale, "production symbol is absent");

    let mut resurrected = load_value();
    resurrected["deleted_symbols"][0]["symbol"] =
        Value::String("pub struct PublishedWorkbench".into());
    assert_rejected(&resurrected, "deleted symbol exists");

    let mut restored_path = load_value();
    restored_path["deleted_paths"][0]["path"] = Value::String("src/startup.rs".into());
    assert_rejected(&restored_path, "deleted path exists");
}

fn assert_valid(value: &Value) {
    let evidence = parse(value);
    let errors = validate(&evidence);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

fn assert_rejected(value: &Value, expected: &str) {
    let evidence = parse(value);
    let errors = validate(&evidence);
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected {expected:?}, errors={errors:?}"
    );
}

fn parse(value: &Value) -> Evidence {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("parse {EVIDENCE_PATH}: {error}"))
}

fn load_value() -> Value {
    serde_json::from_str(&read_repo(EVIDENCE_PATH))
        .unwrap_or_else(|error| panic!("parse {EVIDENCE_PATH}: {error}"))
}

fn validate(evidence: &Evidence) -> Vec<String> {
    let mut errors = Vec::new();
    if evidence.schema != 1 || evidence.issue != 704 {
        errors.push("schema/issue identity differs".into());
    }
    let actual_criteria: Vec<&str> = evidence.criteria.keys().map(String::as_str).collect();
    if actual_criteria != CRITERIA {
        errors.push("criteria keys differ from CWR1-00..CWR1-10".into());
    }
    validate_artifacts(evidence, &mut errors);
    validate_criteria(evidence, &mut errors);
    validate_symbols(evidence, &mut errors);
    validate_deletions(evidence, &mut errors);
    errors
}

fn validate_artifacts(evidence: &Evidence, errors: &mut Vec<String>) {
    let mut previous = "";
    for artifact in &evidence.artifacts {
        if artifact.path.as_str() <= previous {
            errors.push(format!(
                "artifact paths are not sorted and unique: {}",
                artifact.path
            ));
        }
        previous = &artifact.path;
        if artifact.mode != "100644" {
            errors.push(format!("{} mode is not 100644", artifact.path));
        }
        validate_criteria_list(&artifact.criteria, &artifact.path, errors);
        let path = repo_path(&artifact.path);
        match fs::read(&path) {
            Ok(bytes) if hex_sha256(&bytes) == artifact.sha256 => {}
            Ok(_) => errors.push(format!("{} hash differs", artifact.path)),
            Err(error) => errors.push(format!("{} cannot be read: {error}", artifact.path)),
        }
    }
}

fn validate_criteria(evidence: &Evidence, errors: &mut Vec<String>) {
    let artifacts: BTreeSet<&str> = evidence
        .artifacts
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    let approved_commands: BTreeSet<&str> = COMMANDS
        .into_iter()
        .chain(std::iter::once(STATIC_FAILURE_COMMAND))
        .collect();
    for (id, criterion) in &evidence.criteria {
        validate_sorted_unique(&criterion.paths, &format!("{id} paths"), errors);
        let mut previous_command = "";
        for owner in &criterion.commands {
            if owner.command.as_str() <= previous_command {
                errors.push(format!("{id} commands are not sorted and unique"));
            }
            previous_command = &owner.command;
            validate_command_owner(id, owner, &approved_commands, errors);
        }
        for path in &criterion.paths {
            if !artifacts.contains(path.as_str()) {
                errors.push(format!("{id} path is not hashed: {path}"));
            }
        }
        if criterion.paths.is_empty() || criterion.commands.is_empty() {
            errors.push(format!("{id} evidence projection is empty"));
        }
    }
}

fn validate_command_owner(
    id: &str,
    owner: &CommandOwner,
    approved_commands: &BTreeSet<&str>,
    errors: &mut Vec<String>,
) {
    if !approved_commands.contains(owner.command.as_str()) {
        errors.push(format!("{id} command is not approved: {}", owner.command));
    }
    validate_sorted_unique(&owner.platforms, &format!("{id} command platforms"), errors);
    let platforms: Vec<&str> = owner.platforms.iter().map(String::as_str).collect();
    if platforms.is_empty()
        || platforms
            .iter()
            .any(|platform| !PLATFORMS.contains(platform))
    {
        errors.push(format!("{id} platforms must be sorted, unique, and closed"));
    }
    if let Some(path) = owner.command.strip_prefix("tmux_scenario --scenario ") {
        let expected = read_scenario_platform(path, errors);
        if expected.as_ref().is_some_and(|platform| {
            owner.platforms.len() != 1 || owner.platforms.first() != Some(platform)
        }) {
            errors.push(format!(
                "{id} tmux command platform differs from its scenario: {}",
                owner.command
            ));
        }
    } else if owner.command.contains("--target x86_64-pc-windows-msvc")
        && (owner.platforms.len() != 1
            || owner
                .platforms
                .first()
                .is_none_or(|platform| platform != "windows"))
    {
        errors.push(format!(
            "{id} Windows target check has non-Windows ownership"
        ));
    }
}

fn read_scenario_platform(path: &str, errors: &mut Vec<String>) -> Option<String> {
    let source = read_repo(path);
    if let Some(platform) = serde_json::from_str::<Value>(&source)
        .ok()
        .and_then(|value| value["platform"].as_str().map(str::to_owned))
    {
        Some(platform)
    } else {
        errors.push(format!("tmux scenario has no platform: {path}"));
        None
    }
}

fn validate_symbols(evidence: &Evidence, errors: &mut Vec<String>) {
    for owner in &evidence.production_symbols {
        validate_criteria_list(&owner.criteria, &owner.symbol, errors);
        let source = read_repo(&owner.path);
        if !source.contains(&owner.symbol) {
            errors.push(format!(
                "production symbol is absent: {} in {}",
                owner.symbol, owner.path
            ));
        }
    }
    for criterion in CRITERIA {
        if !evidence
            .production_symbols
            .iter()
            .any(|owner| owner.criteria.iter().any(|item| item == criterion))
        {
            errors.push(format!("{criterion} has no production symbol owner"));
        }
    }
}

fn validate_deletions(evidence: &Evidence, errors: &mut Vec<String>) {
    for deletion in &evidence.deleted_symbols {
        validate_criteria_list(&deletion.criteria, &deletion.symbol, errors);
        let root = repo_path(&deletion.scope);
        for path in rust_files(&root) {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            if source.contains(&deletion.symbol) {
                errors.push(format!(
                    "deleted symbol exists: {} in {}",
                    deletion.symbol,
                    path.display()
                ));
            }
        }
    }
    for deletion in &evidence.deleted_paths {
        validate_criteria_list(&deletion.criteria, &deletion.path, errors);
        if repo_path(&deletion.path).exists() {
            errors.push(format!("deleted path exists: {}", deletion.path));
        }
    }
}

fn validate_criteria_list(criteria: &[String], owner: &str, errors: &mut Vec<String>) {
    validate_sorted_unique(criteria, &format!("{owner} criteria"), errors);
    for criterion in criteria {
        if !CRITERIA.contains(&criterion.as_str()) {
            errors.push(format!("{owner} has unknown criterion {criterion}"));
        }
    }
}

fn validate_sorted_unique(values: &[String], label: &str, errors: &mut Vec<String>) {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        errors.push(format!("{label} must be sorted and unique"));
    }
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        {
            let path = entry
                .unwrap_or_else(|error| panic!("read directory entry: {error}"))
                .path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}

fn read_repo(path: &str) -> String {
    fs::read_to_string(repo_path(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).to_string()
}

fn remove_object_key(value: &mut Value, key: &str) {
    value
        .as_object_mut()
        .unwrap_or_else(|| panic!("expected object"))
        .remove(key);
}

fn push_array(value: &mut Value, item: Value) {
    value
        .as_array_mut()
        .unwrap_or_else(|| panic!("expected array"))
        .push(item);
}
