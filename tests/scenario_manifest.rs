//! Executable contracts for the schema-1 scenario evidence manifest (#397).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jefe::domain::sha256::Sha256;
use jefe::harness::v1::contract::Platform;
use jefe::harness::v1::parse_scenario_v1;
use jefe::harness::v1::validate::is_valid_id;
use serde::Deserialize;
use serde_json::Value;

const MANIFEST_PATH: &str = "dev-docs/testing/scenario-execution-manifest.json";
const OWNER_EVIDENCE_PATH: &str = "dev-docs/testing/scenario-owner-evidence.json";
const SCENARIO_ROOT: &str = "dev-docs/tmux-scenarios";
const WINDOWS_REASON: &str = "schema-1 platform grammar admits only macos|linux (src/harness/v1/contract.rs::Platform); the surviving runner requires a Unix PTY";
const ISSUE493_PATH: &str = "dev-docs/tmux-scenarios/v1/issue493-server-loss.json";
const ISSUE493_UNIX_REASON: &str = "issue493 exercises Windows psmux shared-server loss; Unix runtimes reconcile individual sessions and cannot produce ServerLost";
const CRITERIA: [&str; 8] = [
    "CW00B-01", "CW00B-02", "CW00B-03", "CW00B-04", "CW00B-05", "CW00B-06", "CW00B-07", "CW00B-08",
];
const ASSERTION_OPS: [&str; 3] = ["assert-frame", "assert-capture", "assert-file"];

#[cfg(unix)]
trait TestResult<T> {
    fn must(self, context: &str) -> T;
}

#[cfg(unix)]
impl<T, E: std::fmt::Display> TestResult<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|err| panic!("{context}: {err}"))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionManifest {
    schema: u32,
    scenarios: Vec<ScenarioEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioEntry {
    path: String,
    scenario_schema: u32,
    criteria: Vec<String>,
    platforms: BTreeMap<String, PlatformDisposition>,
    ci_job: String,
    command: ScenarioCommand,
    timeout_ms: u64,
    expect: ExpectedEvidence,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlatformDisposition {
    disposition: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCommand {
    binary: String,
    installs: Vec<Install>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Install {
    name: String,
    source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedEvidence {
    exit_code: u8,
    report_status: String,
    steps_total: usize,
    operations: Vec<String>,
    assertions: BTreeMap<String, usize>,
    captures: usize,
    capture_names: Vec<String>,
    failed_step: Option<ExpectedFailedStep>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedFailedStep {
    index: usize,
    op: String,
    error_prefix: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerEvidence {
    schema: u32,
    scenario_manifest_sha256: String,
    scenarios: Vec<OwnedScenario>,
    artifacts: Vec<OwnedPath>,
    harness_modules: Vec<HarnessModule>,
    deleted_paths: Vec<DeletedPath>,
    criteria: BTreeMap<String, CriterionEvidence>,
    production_symbols: Vec<String>,
    deleted_symbols: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedScenario {
    path: String,
    sha256: String,
    mode: String,
    criteria: Vec<String>,
    platform: String,
    ci_job: String,
    command: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedPath {
    path: String,
    sha256: String,
    mode: String,
    criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HarnessModule {
    path: String,
    sha256: String,
    mode: String,
    criteria: Vec<String>,
    classification: String,
    imported_by: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletedPath {
    path: String,
    sha256: String,
    mode: String,
    criteria: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CriterionEvidence {
    paths: Vec<String>,
    commands: Vec<String>,
    platforms: Vec<String>,
    production_symbols: Vec<String>,
    deleted_symbols: Vec<String>,
}

#[test]
fn manifest_exactly_classifies_the_recursive_corpus() {
    let manifest = load_manifest();
    validate_manifest(&manifest).unwrap_or_else(|errors| panic!("{}", errors.join("\n")));
}

#[test]
fn owner_evidence_matches_the_active_manifest() {
    let manifest = load_manifest();
    let evidence: OwnerEvidence = serde_json::from_str(&read_repo_text(OWNER_EVIDENCE_PATH))
        .unwrap_or_else(|err| panic!("parse {OWNER_EVIDENCE_PATH}: {err}"));
    validate_owner_evidence(&manifest, &evidence)
        .unwrap_or_else(|errors| panic!("{}", errors.join("\n")));
}

#[test]
fn ci_executes_and_accounts_for_every_deterministic_scenario_shard() {
    let workflow = read_repo_text(".github/workflows/ci.yml");
    for required in [
        "tui_scenarios_linux:",
        "shard: [0, 1]",
        "--platform linux \\",
        "--shard-count 2 \\",
        "--verify-completion target/tmux-scenarios/linux",
        "--expected-shards 2",
        "tui_scenarios_macos:",
        "shard: [0, 1, 2, 3, 4, 5]",
        "--platform macos \\",
        "--shard-count 6 \\",
        "--verify-completion target/tmux-scenarios/macos",
        "--expected-shards 6",
        "tui_scenarios_complete:",
        "\"tui_scenarios_linux\": \"${{ needs.tui_scenarios_linux.result }}\"",
        "\"tui_scenarios_macos\": \"${{ needs.tui_scenarios_macos.result }}\"",
        "\"tui_scenarios_complete\": \"${{ needs.tui_scenarios_complete.result }}\"",
    ] {
        assert!(
            workflow.contains(required),
            "CI lacks exact shard contract: {required}"
        );
    }
}

fn validate_manifest(manifest: &ExecutionManifest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if manifest.schema != 1 {
        errors.push("manifest schema must be 1".to_string());
    }

    let actual_paths = shipped_scenario_paths();
    let manifest_paths: Vec<_> = manifest
        .scenarios
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    if !manifest_paths.windows(2).all(|pair| pair[0] < pair[1]) {
        errors.push("scenario paths must be strictly sorted and unique".to_string());
    }
    let actual_set: BTreeSet<_> = actual_paths.iter().cloned().collect();
    let manifest_set: BTreeSet<_> = manifest_paths.iter().cloned().collect();
    if actual_set != manifest_set || manifest_set.len() != manifest_paths.len() {
        errors.push(format!(
            "manifest paths differ from shipped scenario paths: manifest={}, shipped={}",
            manifest_paths.len(),
            actual_paths.len()
        ));
    }

    let workflow = read_repo_text(".github/workflows/ci.yml");
    for entry in &manifest.scenarios {
        validate_entry(entry, &workflow, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_entry(entry: &ScenarioEntry, workflow: &str, errors: &mut Vec<String>) {
    let Some((bytes, raw)) = read_scenario(entry, errors) else {
        return;
    };
    let scenario_schema = raw.get("schema").and_then(Value::as_u64);
    if scenario_schema != Some(u64::from(entry.scenario_schema)) {
        errors.push(format!("{} scenario_schema differs from file", entry.path));
    }
    let scenario = match parse_scenario_v1(&bytes) {
        Ok(scenario) => Some(scenario),
        Err(err) => {
            errors.push(format!("{} must parse as schema 1: {err}", entry.path));
            None
        }
    };

    validate_criteria(entry, errors);
    validate_platforms(entry, parsed_platform(&raw), errors);
    validate_command(entry, errors);
    validate_expected_inventory(entry, &raw, errors);
    if !(1..=600_000).contains(&entry.timeout_ms) {
        errors.push(format!("{} timeout_ms is outside 1..=600000", entry.path));
    }
    if !workflow.contains(&format!("  {}:", entry.ci_job)) {
        errors.push(format!(
            "{} CI job {} does not exist",
            entry.path, entry.ci_job
        ));
    }
    if let Some(scenario) = scenario {
        let expected_job = if entry.path == ISSUE493_PATH {
            "windows_native"
        } else {
            match scenario.platform {
                Platform::Macos => "tui_scenarios_macos",
                Platform::Linux => "tui_scenarios_linux",
            }
        };
        if entry.ci_job != expected_job {
            errors.push(format!(
                "{} CI job must be {expected_job}, got {}",
                entry.path, entry.ci_job
            ));
        }
    }
}

fn read_scenario(entry: &ScenarioEntry, errors: &mut Vec<String>) -> Option<(Vec<u8>, Value)> {
    let path = repo_path(&entry.path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            errors.push(format!("{} cannot be read: {err}", entry.path));
            return None;
        }
    };
    let raw = match serde_json::from_slice(&bytes) {
        Ok(raw) => raw,
        Err(err) => {
            errors.push(format!("{} is not JSON: {err}", entry.path));
            return None;
        }
    };
    Some((bytes, raw))
}

fn validate_criteria(entry: &ScenarioEntry, errors: &mut Vec<String>) {
    if entry.criteria.is_empty()
        || !entry.criteria.windows(2).all(|pair| pair[0] < pair[1])
        || entry
            .criteria
            .iter()
            .any(|criterion| !CRITERIA.contains(&criterion.as_str()))
    {
        errors.push(format!(
            "{} criteria must be nonempty, sorted, unique, and closed",
            entry.path
        ));
    }
}

fn validate_platforms(entry: &ScenarioEntry, declared: Option<&str>, errors: &mut Vec<String>) {
    let expected_names = BTreeSet::from(["linux", "macos", "windows"]);
    let actual_names: BTreeSet<_> = entry.platforms.keys().map(String::as_str).collect();
    if actual_names != expected_names {
        errors.push(format!(
            "{} must classify linux, macos, and windows",
            entry.path
        ));
        return;
    }
    let required: Vec<_> = entry
        .platforms
        .iter()
        .filter(|(_, value)| value.disposition == "required")
        .map(|(name, _)| name.as_str())
        .collect();
    if entry.path == ISSUE493_PATH {
        if !required.is_empty()
            || entry.platforms["linux"].reason.as_deref() != Some(ISSUE493_UNIX_REASON)
            || entry.platforms["macos"].reason.as_deref() != Some(ISSUE493_UNIX_REASON)
            || entry.ci_job != "windows_native"
        {
            errors.push(format!(
                "{} must be excluded from Unix execution and owned by native Windows psmux evidence",
                entry.path
            ));
        }
    } else if required.as_slice() != declared.into_iter().collect::<Vec<_>>().as_slice() {
        errors.push(format!(
            "{} required platform differs from scenario declaration",
            entry.path
        ));
    }
    for (name, value) in &entry.platforms {
        match value.disposition.as_str() {
            "required" if value.reason.is_none() => {}
            "unsupported"
                if value
                    .reason
                    .as_deref()
                    .is_some_and(|reason| !reason.is_empty()) => {}
            "unsupported" => errors.push(format!(
                "{} unsupported {name} disposition needs a reason",
                entry.path
            )),
            other => errors.push(format!(
                "{} has invalid {name} disposition {other}",
                entry.path
            )),
        }
    }
    if entry.platforms["windows"].reason.as_deref() != Some(WINDOWS_REASON) {
        errors.push(format!(
            "{} windows unsupported reason is not deterministic",
            entry.path
        ));
    }
}

fn validate_command(entry: &ScenarioEntry, errors: &mut Vec<String>) {
    if entry.command.binary != "tmux_scenario" {
        errors.push(format!(
            "{} command binary must be tmux_scenario",
            entry.path
        ));
    }
    if !entry
        .command
        .installs
        .windows(2)
        .all(|pair| pair[0].name < pair[1].name)
    {
        errors.push(format!("{} installs must be sorted and unique", entry.path));
    }
    for install in &entry.command.installs {
        if !is_valid_id(&install.name) {
            errors.push(format!(
                "{} install {} has invalid name",
                entry.path, install.name
            ));
        }
        let valid = install.source.starts_with("cargo-bin:")
            || install.source.starts_with("repo:")
            || install.source.starts_with("host-path:");
        if !valid {
            errors.push(format!(
                "{} install {} has invalid source {}",
                entry.path, install.name, install.source
            ));
        }
        if let Some(relative) = install.source.strip_prefix("repo:")
            && !repo_path(relative).is_file()
        {
            errors.push(format!(
                "{} install {} source does not exist",
                entry.path, install.name
            ));
        }
    }
}

fn validate_expected_inventory(entry: &ScenarioEntry, raw: &Value, errors: &mut Vec<String>) {
    let steps = raw
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let operations: Vec<_> = steps
        .iter()
        .filter_map(|step| step.get("op").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let unique_operations: BTreeSet<_> = operations.iter().cloned().collect();
    let expected_operations: BTreeSet<_> = entry.expect.operations.iter().cloned().collect();
    if steps.len() != entry.expect.steps_total {
        errors.push(format!("{} step count differs from scenario", entry.path));
    }
    if unique_operations != expected_operations
        || entry
            .expect
            .operations
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        errors.push(format!(
            "{} operation inventory differs from scenario",
            entry.path
        ));
    }
    let capture_names: Vec<_> = steps
        .iter()
        .filter(|step| step.get("op").and_then(Value::as_str) == Some("capture"))
        .filter_map(|step| step.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if capture_names.len() != entry.expect.captures || capture_names != entry.expect.capture_names {
        errors.push(format!(
            "{} capture inventory differs from scenario",
            entry.path
        ));
    }
    validate_failure_expectation(entry, &operations, errors);
    let expected_assertions: BTreeMap<_, _> = ASSERTION_OPS
        .iter()
        .filter_map(|op| {
            let count = operations
                .iter()
                .filter(|actual| actual.as_str() == *op)
                .count();
            (count > 0).then_some(((*op).to_string(), count))
        })
        .collect();
    if entry.expect.assertions != expected_assertions {
        errors.push(format!(
            "{} assertion inventory differs from scenario",
            entry.path
        ));
    }
}

fn validate_failure_expectation(
    entry: &ScenarioEntry,
    operations: &[String],
    errors: &mut Vec<String>,
) {
    let valid = match entry.expect.exit_code {
        0 => entry.expect.report_status == "passed" && entry.expect.failed_step.is_none(),
        3 | 4 | 124 => {
            entry.expect.report_status == "failed"
                && entry.expect.failed_step.as_ref().is_some_and(|failure| {
                    operations.get(failure.index) == Some(&failure.op)
                        && failure.error_prefix.starts_with("HAR-E")
                })
        }
        _ => false,
    };
    if !valid {
        errors.push(format!(
            "{} failure expectation differs from scenario",
            entry.path
        ));
    }
}

fn validate_owner_evidence(
    manifest: &ExecutionManifest,
    evidence: &OwnerEvidence,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if evidence.schema != 1 {
        errors.push("owner evidence schema must be 1".to_string());
    }
    let manifest_bytes = std::fs::read(repo_path(MANIFEST_PATH))
        .unwrap_or_else(|err| panic!("read {MANIFEST_PATH}: {err}"));
    if evidence.scenario_manifest_sha256 != Sha256::digest(&manifest_bytes).to_string() {
        errors.push("owner evidence manifest hash differs".to_string());
    }
    if evidence.scenarios.len() != manifest.scenarios.len() {
        errors.push("owner evidence path count differs".to_string());
    }
    for (entry, owned) in manifest.scenarios.iter().zip(&evidence.scenarios) {
        validate_owned_scenario(entry, owned, &mut errors);
    }
    validate_artifacts(&evidence.artifacts, &mut errors);
    validate_repo_install_artifacts(manifest, &evidence.artifacts, &mut errors);
    validate_harness_modules(&evidence.harness_modules, &mut errors);
    validate_deleted_paths(&evidence.deleted_paths, &mut errors);
    validate_criterion_evidence(evidence, &mut errors);
    if evidence.production_symbols
        != [
            "jefe::harness::v1::parse_scenario_v1",
            "jefe::harness::v1::runner::run",
            "tmux_scenario",
        ]
        || evidence.deleted_symbols
            != [
                concat!("Psmux", "Driver"),
                concat!("Tmux", "Driver"),
                concat!("jefe-tmux", "-harness"),
                concat!("run_tmux", "_v1"),
            ]
    {
        errors.push("owner symbol inventories differ from the sole authority".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_owned_scenario(entry: &ScenarioEntry, owned: &OwnedScenario, errors: &mut Vec<String>) {
    if owned.path != entry.path {
        errors.push(format!("{} owner path/order differs", entry.path));
        return;
    }
    let scenario_bytes = std::fs::read(repo_path(&entry.path))
        .unwrap_or_else(|err| panic!("read {}: {err}", entry.path));
    if owned.sha256 != Sha256::digest(&scenario_bytes).to_string() {
        errors.push(format!("{} owner hash differs", entry.path));
    }
    validate_file_mode(&entry.path, &owned.mode, errors);
    let platform = entry
        .platforms
        .iter()
        .find(|(_, disposition)| disposition.disposition == "required")
        .map(|(name, _)| name.as_str())
        .or_else(|| (entry.path == ISSUE493_PATH).then_some("windows"));
    if platform != Some(owned.platform.as_str())
        || owned.ci_job != entry.ci_job
        || owned.criteria != entry.criteria
    {
        errors.push(format!("{} owner classification differs", entry.path));
    }
    let installs = entry
        .command
        .installs
        .iter()
        .fold(String::new(), |mut result, install| {
            use std::fmt::Write as _;
            write!(result, " --install {}={}", install.name, install.source)
                .unwrap_or_else(|err| panic!("format install: {err}"));
            result
        });
    let command = format!("tmux_scenario --scenario {}{installs}", entry.path);
    if owned.command != command {
        errors.push(format!("{} owner command differs", entry.path));
    }
}

fn validate_artifacts(artifacts: &[OwnedPath], errors: &mut Vec<String>) {
    if !is_sorted_unique(artifacts.iter().map(|item| item.path.as_str())) {
        errors.push("owner artifact paths must be sorted and unique".to_string());
    }
    for artifact in artifacts {
        validate_current_path(&artifact.path, &artifact.sha256, &artifact.mode, errors);
        validate_criterion_tags(&artifact.path, &artifact.criteria, errors);
    }
}

fn validate_repo_install_artifacts(
    manifest: &ExecutionManifest,
    artifacts: &[OwnedPath],
    errors: &mut Vec<String>,
) {
    let artifacts = artifacts
        .iter()
        .map(|artifact| (artifact.path.as_str(), &artifact.criteria))
        .collect::<BTreeMap<_, _>>();
    for entry in &manifest.scenarios {
        for install in &entry.command.installs {
            let Some(path) = install.source.strip_prefix("repo:") else {
                continue;
            };
            let Some(criteria) = artifacts.get(path) else {
                errors.push(format!(
                    "{} repo install {path} lacks immutable artifact evidence",
                    entry.path
                ));
                continue;
            };
            if entry
                .criteria
                .iter()
                .any(|criterion| !criteria.contains(criterion))
            {
                errors.push(format!(
                    "{} repo install {path} lacks scenario criterion ownership",
                    entry.path
                ));
            }
        }
    }
}

fn validate_harness_modules(modules: &[HarnessModule], errors: &mut Vec<String>) {
    let mut actual = Vec::new();
    collect_paths_with_extension(&repo_path("src/harness"), "rs", &mut actual);
    let actual = actual
        .into_iter()
        .map(|path| display_repo_path(&path))
        .collect::<Vec<_>>();
    let declared = modules
        .iter()
        .map(|module| module.path.clone())
        .collect::<Vec<_>>();
    if declared != actual {
        errors.push(
            "harness module inventory differs from recursive src/harness/**/*.rs".to_string(),
        );
    }

    for module in modules {
        validate_current_path(&module.path, &module.sha256, &module.mode, errors);
        validate_criterion_tags(&module.path, &module.criteria, errors);
        let expected_classification = if module.path.ends_with("_tests.rs") {
            "test"
        } else {
            "production"
        };
        if module.classification != expected_classification {
            errors.push(format!("{} module classification differs", module.path));
        }
        let importer = repo_path(&module.imported_by);
        if !importer.is_file() {
            errors.push(format!("{} importer is absent", module.path));
            continue;
        }
        let importer_text = std::fs::read_to_string(&importer)
            .unwrap_or_else(|err| panic!("read {}: {err}", importer.display()));
        let file_name = Path::new(&module.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("module path has no file name: {}", module.path));
        let stem = if file_name == "mod.rs" {
            Path::new(&module.path)
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("module path has no parent name: {}", module.path))
        } else {
            file_name.strip_suffix(".rs").unwrap_or(file_name)
        };
        let declared_by_name = importer_text.contains(&format!("mod {stem};"));
        let declared_by_path = importer_text.contains(&format!("#[path = \"{file_name}\"]"));
        if !declared_by_name && !declared_by_path {
            errors.push(format!(
                "{} is not declared by {}",
                module.path, module.imported_by
            ));
        }
    }
}

fn validate_deleted_paths(paths: &[DeletedPath], errors: &mut Vec<String>) {
    if !is_sorted_unique(paths.iter().map(|item| item.path.as_str())) {
        errors.push("deleted paths must be sorted and unique".to_string());
    }
    for path in paths {
        if repo_path(&path.path).exists() {
            errors.push(format!("{} deleted predecessor still exists", path.path));
        }
        if path.sha256.len() != 64 || !path.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            errors.push(format!("{} deleted hash is invalid", path.path));
        }
        if !matches!(path.mode.as_str(), "100644" | "100755") {
            errors.push(format!("{} deleted mode is invalid", path.path));
        }
        validate_criterion_tags(&path.path, &path.criteria, errors);
    }
}

fn indexed_evidence_paths(evidence: &OwnerEvidence) -> BTreeMap<&str, &[String]> {
    evidence
        .scenarios
        .iter()
        .map(|item| (item.path.as_str(), item.criteria.as_slice()))
        .chain(
            evidence
                .artifacts
                .iter()
                .map(|item| (item.path.as_str(), item.criteria.as_slice())),
        )
        .chain(
            evidence
                .harness_modules
                .iter()
                .map(|item| (item.path.as_str(), item.criteria.as_slice())),
        )
        .chain(
            evidence
                .deleted_paths
                .iter()
                .map(|item| (item.path.as_str(), item.criteria.as_slice())),
        )
        .collect()
}

fn validate_criterion_evidence(evidence: &OwnerEvidence, errors: &mut Vec<String>) {
    let expected = (1..=8)
        .map(|number| format!("CW00B-{number:02}"))
        .collect::<Vec<_>>();
    if evidence.criteria.keys().cloned().collect::<Vec<_>>() != expected {
        errors
            .push("criterion evidence must classify exactly CW00B-01 through CW00B-08".to_string());
    }

    let known_paths = indexed_evidence_paths(evidence);

    for (criterion, owner) in &evidence.criteria {
        if owner.paths.is_empty()
            || owner.commands.is_empty()
            || owner.platforms.is_empty()
            || !is_sorted_unique(owner.paths.iter().map(String::as_str))
            || !is_sorted_unique(owner.commands.iter().map(String::as_str))
            || !is_sorted_unique(owner.platforms.iter().map(String::as_str))
            || !is_sorted_unique(owner.production_symbols.iter().map(String::as_str))
            || !is_sorted_unique(owner.deleted_symbols.iter().map(String::as_str))
        {
            errors.push(format!("{criterion} evidence is incomplete or unordered"));
        }
        for path in &owner.paths {
            match known_paths.get(path.as_str()) {
                Some(criteria) if criteria.iter().any(|value| value == criterion) => {}
                _ => errors.push(format!("{criterion} does not own declared path {path}")),
            }
        }
        if owner.commands.iter().any(|command| {
            command.contains(concat!("jefe-tmux", "-harness"))
                || command.contains(concat!("run_tmux", "_v1"))
        }) {
            errors.push(format!("{criterion} names a predecessor command"));
        }
        if owner
            .platforms
            .iter()
            .any(|platform| !matches!(platform.as_str(), "linux" | "macos" | "windows"))
        {
            errors.push(format!("{criterion} has an unknown platform"));
        }
    }

    for (path, criteria) in known_paths {
        for criterion in criteria {
            if !evidence
                .criteria
                .get(criterion)
                .is_some_and(|owner| owner.paths.iter().any(|owned| owned == path))
            {
                errors.push(format!("{path} is not indexed by {criterion}"));
            }
        }
    }
}

fn validate_current_path(path: &str, sha256: &str, mode: &str, errors: &mut Vec<String>) {
    let bytes = match std::fs::read(repo_path(path)) {
        Ok(bytes) => bytes,
        Err(err) => {
            errors.push(format!("{path} owner path is unreadable: {err}"));
            return;
        }
    };
    if sha256 != Sha256::digest(&bytes).to_string() {
        errors.push(format!("{path} owner hash differs"));
    }
    validate_file_mode(path, mode, errors);
}

#[cfg(unix)]
fn validate_file_mode(path: &str, mode: &str, errors: &mut Vec<String>) {
    use std::os::unix::fs::PermissionsExt as _;

    let permissions = std::fs::metadata(repo_path(path))
        .unwrap_or_else(|err| panic!("metadata {path}: {err}"))
        .permissions()
        .mode();
    let actual = if permissions & 0o111 == 0 {
        "100644"
    } else {
        "100755"
    };
    if mode != actual {
        errors.push(format!("{path} owner mode differs"));
    }
}

#[cfg(not(unix))]
fn validate_file_mode(path: &str, mode: &str, errors: &mut Vec<String>) {
    if !matches!(mode, "100644" | "100755") {
        errors.push(format!("{path} owner mode is invalid"));
    }
}

fn validate_criterion_tags(path: &str, criteria: &[String], errors: &mut Vec<String>) {
    if criteria.is_empty()
        || !is_sorted_unique(criteria.iter().map(String::as_str))
        || criteria
            .iter()
            .any(|criterion| !criterion.starts_with("CW00B-"))
    {
        errors.push(format!("{path} criteria are incomplete or unordered"));
    }
}

fn is_sorted_unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let values = values.collect::<Vec<_>>();
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn display_repo_path(path: &Path) -> String {
    path.strip_prefix(repo_path(""))
        .unwrap_or_else(|err| panic!("strip repository prefix: {err}"))
        .to_string_lossy()
        .replace('\\', "/")
}

fn collect_paths_with_extension(directory: &Path, extension: &str, paths: &mut Vec<PathBuf>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("read entry in {}: {err}", directory.display()))
                .path()
        })
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_paths_with_extension(&path, extension, paths);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            paths.push(path);
        }
    }
}

fn parsed_platform(raw: &Value) -> Option<&str> {
    raw.get("platform").and_then(Value::as_str)
}

fn assert_validation_contains(manifest: &ExecutionManifest, expected: &str) {
    let errors = match validate_manifest(manifest) {
        Ok(()) => panic!("mutated manifest must fail"),
        Err(errors) => errors,
    };
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected {expected:?} in {errors:?}"
    );
}

fn load_manifest() -> ExecutionManifest {
    let text = read_repo_text(MANIFEST_PATH);
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("parse {MANIFEST_PATH}: {err}"))
}

fn shipped_scenario_paths() -> Vec<String> {
    let root = repo_path(SCENARIO_ROOT);
    let mut paths = Vec::new();
    collect_json_paths(&root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            path.strip_prefix(repo_path(""))
                .unwrap_or_else(|err| panic!("strip repository prefix: {err}"))
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn collect_json_paths(directory: &Path, paths: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("read entry in {}: {err}", directory.display()))
            .path();
        if path.is_dir() {
            collect_json_paths(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
}

fn read_repo_text(relative: impl AsRef<Path>) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn repo_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[path = "scenario_manifest/projection_tests.rs"]
mod projection_tests;

#[cfg(unix)]
#[path = "scenario_manifest/driver_tests.rs"]
mod driver_tests;
