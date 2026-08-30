//! Immutable acceptance-owner evidence for issue #705 (CWR2-00..CWR2-11, including CWR2-01A).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use jefe::domain::sha256::Sha256;
use serde::Deserialize;
use serde_json::Value;

#[path = "support/issue705_scenarios.rs"]
mod issue705_scenarios;

const EVIDENCE_PATH: &str = "dev-docs/testing/issue705-owner-evidence.json";
const VALIDATED_BASE_REVISION: &str = "a020ea6edf3f2b71d8cad1f7895850b5b8c96eb9";
const CRITERIA: [&str; 13] = [
    "CWR2-00", "CWR2-01", "CWR2-01A", "CWR2-02", "CWR2-03", "CWR2-04", "CWR2-05", "CWR2-06",
    "CWR2-07", "CWR2-08", "CWR2-09", "CWR2-10", "CWR2-11",
];
const PLATFORMS: [&str; 3] = ["linux", "macos", "windows"];
const COMMANDS: [&str; 19] = [
    "cargo build --workspace --all-features --locked",
    "cargo check --workspace --all-targets --all-features --locked --target x86_64-pc-windows-msvc",
    "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
    "cargo fmt --all --check",
    "cargo test --locked --all-features --test issue705_owner_evidence",
    "cargo test --workspace --all-targets --all-features --locked",
    "cargo xtask check source-size",
    "cargo xtask ci",
    "cargo xtask quick",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/dashboard-parity-linux.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/dashboard-parity-macos.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/provider-health-linux.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/provider-health-macos.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/semantic-continuation-linux.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/semantic-continuation-macos.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/tree-structured-diff-linux.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/tree-structured-diff-macos.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/workbench-runtime-linux.json",
    "tmux_scenario --scenario dev-docs/tmux-scenarios/issue705/workbench-runtime-macos.json",
];
const REQUIRED_ARTIFACT_PATHS: [&str; 16] = [
    "dev-docs/testing/issue704-owner-evidence.json",
    "dev-docs/testing/scenario-execution-manifest.json",
    "dev-docs/testing/scenario-owner-evidence.json",
    "src/app_shell.rs",
    "src/host_panel_models.rs",
    "src/mouse_action_routing.rs",
    "src/mouse_routing.rs",
    "src/screen_layout.rs",
    "src/state/host_panel_input_ops.rs",
    "src/state/settings_registry_provider_health_tests.rs",
    "src/state/types.rs",
    "src/ui/components/keybind_bar.rs",
    "src/ui/orchestration.rs",
    "src/workbench/compose_fixtures.rs",
    "src/workbench/testdata/local-control-origin.screen.toml",
    "tests/support/issue705_scenarios.rs",
];
const REQUIRED_DELETED_PATHS: [&str; 7] = [
    "src/app_input/dashboard_search.rs",
    "src/state/dashboard_search_ops.rs",
    "src/ui/modals/confirm.rs",
    "src/ui/modals/help.rs",
    "src/ui/modals/help_scroll_tests.rs",
    "src/ui/modals/provider.rs",
    "src/ui/screens/dashboard.rs",
];
const REQUIRED_DELETED_SYMBOLS: [(&str, &str); 6] = [
    ("src", "HostControlKind"),
    ("src", "PriorAgentFocus"),
    ("src", "ScreenId::Dashboard"),
    ("src", "Subject(String)"),
    ("src", "detail_target_for"),
    ("src", "master_detail_edge"),
];
const REQUIRED_RESIDUAL_ADAPTERS: [(&str, &str); 7] = [
    ("Repositories", "src/workbench/screens.rs"),
    ("Issues", "src/workbench/screens.rs"),
    ("PullRequests", "src/workbench/screens.rs"),
    ("Actions", "src/workbench/screens.rs"),
    ("Errors", "src/workbench/screens.rs"),
    ("Terminals", "src/workbench/screens.rs"),
    ("Settings", "src/workbench/screens.rs"),
];
const REQUIRED_PRODUCTION_SYMBOLS: [(&str, &str); 22] = [
    (
        "src/domain/action_registry.rs",
        "pub struct ActionRegistrySnapshot",
    ),
    ("src/host_controls.rs", "pub enum ControlKind"),
    (
        "src/host_controls.rs",
        "pub(crate) trait HostControlFactory",
    ),
    ("src/overlay_controls.rs", "pub struct HostOverlayLayout"),
    (
        "src/provider_panel_view.rs",
        "pub fn project_current_screen",
    ),
    (
        "src/published_workbench.rs",
        "pub struct PublishedWorkbench",
    ),
    ("src/runtime/provider/panel_model.rs", "pub struct TreeNode"),
    (
        "src/runtime/provider/persistent_session.rs",
        "pub struct PersistentSessionOwner",
    ),
    (
        "src/state/navigation.rs",
        "pub struct InstancePresentationState",
    ),
    ("src/state/navigation.rs", "pub struct ScreenInstance"),
    ("src/state/navigation_unwind.rs", "pub fn resolve_back"),
    (
        "src/state/overlay_projection_ops.rs",
        "pub fn help_control_scroll",
    ),
    (
        "src/state/provider_action_context.rs",
        "pub fn project_current_context",
    ),
    (
        "src/state/provider_panels.rs",
        "pub struct ProviderPanelState",
    ),
    ("src/state/provider_view.rs", "pub struct ProviderViewInput"),
    (
        "src/state/screen_overlays.rs",
        "pub struct ScreenOverlayState",
    ),
    ("src/state/types.rs", "pub fn publish_resolved_layout"),
    (
        "src/ui/orchestration.rs",
        "pub fn confirmation_hit_target_at_content_line",
    ),
    (
        "src/workbench/compose.rs",
        "fn bind_selected_provider_panels",
    ),
    (
        "src/workbench/relationship_propagation.rs",
        "pub struct RelationshipInstance",
    ),
    (
        "src/workbench/resource_schemas.rs",
        "pub struct ResourceSchemaRegistry",
    ),
    ("src/workbench/screens.rs", "pub struct ScreenRegistry"),
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    schema: u32,
    issue: u32,
    validation: Validation,
    artifacts: Vec<Artifact>,
    criteria: BTreeMap<String, Criterion>,
    production_symbols: Vec<SymbolOwner>,
    deleted_symbols: Vec<DeletedSymbol>,
    deleted_paths: Vec<DeletedPath>,
    residual_adapters: Vec<ResidualAdapter>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Validation {
    phase: String,
    status: String,
    base_revision: String,
    artifact_set_sha256: String,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResidualAdapter {
    screen: String,
    path: String,
    criteria: Vec<String>,
}

#[test]
fn issue705_owner_evidence_is_exact_and_current() {
    assert_valid(&load_value());
}

#[test]
fn stale_artifact_hash_is_rejected() {
    let mut value = load_value();
    value["artifacts"][0]["sha256"] = Value::String("0".repeat(64));
    assert_rejected(&value, "hash differs");
}

#[test]
fn non_green_or_mismatched_exact_head_validation_is_rejected() {
    let mut non_green = load_value();
    non_green["validation"]["status"] = Value::String("PENDING".into());
    assert_rejected(&non_green, "exact-head validation status differs");

    let mut wrong_revision = load_value();
    wrong_revision["validation"]["base_revision"] = Value::String("0".repeat(40));
    assert_rejected(&wrong_revision, "validated base revision differs");

    let mut wrong_artifacts = load_value();
    wrong_artifacts["validation"]["artifact_set_sha256"] = Value::String("0".repeat(64));
    assert_rejected(&wrong_artifacts, "validated artifact set hash differs");
}

#[test]
fn artifact_hashes_normalize_checkout_line_endings() {
    assert_eq!(
        hex_sha256(b"alpha\nbeta\n"),
        hex_sha256(b"alpha\r\nbeta\r\n")
    );
}
#[test]
fn missing_or_duplicate_criteria_are_rejected() {
    let mut missing = load_value();
    remove_object_key(&mut missing["criteria"], "CWR2-10");
    assert_rejected(&missing, "criteria keys differ");

    let mut duplicate = load_value();
    let criterion = duplicate["artifacts"][0]["criteria"][0].clone();
    push_array(&mut duplicate["artifacts"][0]["criteria"], criterion);
    assert_rejected(&duplicate, "criteria must be sorted and unique");
}

#[test]
fn invalid_command_or_platform_is_rejected() {
    let mut command = load_value();
    command["criteria"]["CWR2-00"]["commands"][0]["command"] =
        Value::String("cargo test --ignored".into());
    assert_rejected(&command, "command is not approved");

    let mut platform = load_value();
    platform["criteria"]["CWR2-00"]["commands"][0]["platforms"][0] =
        Value::String("solaris".into());
    assert_rejected(&platform, "platforms must be sorted, unique, and closed");

    let mut unsupported_tmux = load_value();
    unsupported_tmux["criteria"]["CWR2-01"]["commands"][1]["platforms"] =
        serde_json::json!(["macos", "windows"]);
    assert_rejected(
        &unsupported_tmux,
        "tmux command platform differs from its scenario",
    );
}

#[test]
fn stale_symbol_and_resurrected_authority_are_rejected() {
    let mut stale = load_value();
    stale["production_symbols"][0]["symbol"] = Value::String("missing_issue705_symbol".into());
    assert_rejected(&stale, "production symbol is absent");

    let mut resurrected = load_value();
    resurrected["deleted_symbols"][0]["symbol"] =
        Value::String("pub struct PublishedWorkbench".into());
    assert_rejected(&resurrected, "deleted symbol exists");

    let mut restored_path = load_value();
    restored_path["deleted_paths"][0]["path"] = Value::String("src/startup.rs".into());
    assert_rejected(&restored_path, "deleted path exists");

    let mut omitted_path = load_value();
    omitted_path["deleted_paths"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("deleted_paths must be an array"))
        .pop();
    assert_rejected(&omitted_path, "deleted path ledger differs");
}
#[test]
fn omitted_deleted_symbol_or_residual_adapter_is_rejected() {
    let mut omitted_symbol = load_value();
    omitted_symbol["deleted_symbols"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("deleted_symbols must be an array"))
        .pop();
    assert_rejected(&omitted_symbol, "deleted symbol ledger differs");

    let mut omitted_residual = load_value();
    omitted_residual["residual_adapters"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("residual_adapters must be an array"))
        .pop();
    assert_rejected(&omitted_residual, "residual adapter ledger differs");
}

#[test]
fn mirrored_issue705_scenarios_are_complete_and_semantically_paired() {
    let errors = issue705_scenarios::validate_scenario_pairs();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn semantic_continuation_is_a_required_mirrored_pair() {
    assert!(issue705_scenarios::semantic_continuation_pair_is_required());
}

#[test]
fn one_sided_semantic_continuation_drift_is_rejected() {
    assert!(issue705_scenarios::one_sided_semantic_continuation_drift_is_detected());
}

#[test]
fn omitted_required_runtime_or_nested_evidence_artifact_is_rejected() {
    let mut value = load_value();
    let artifacts = value["artifacts"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("artifacts must be an array"));
    artifacts.retain(|artifact| artifact["path"] != REQUIRED_ARTIFACT_PATHS[0]);
    assert_rejected(&value, "required artifact ledger is incomplete");
}

#[test]
fn omitted_owner_and_one_sided_criterion_mapping_are_rejected() {
    let mut omitted_owner = load_value();
    omitted_owner["production_symbols"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("production_symbols must be an array"))
        .pop();
    assert_rejected(&omitted_owner, "production symbol ledger differs");

    let mut one_sided = load_value();
    one_sided["artifacts"][0]["criteria"] = serde_json::json!([]);
    assert_rejected(&one_sided, "path lacks matching artifact ownership");
}

#[test]
fn dashboard_product_selectors_are_confined_to_model_ownership_and_descriptors() {
    const PRODUCT_TYPES: [&str; 4] = [
        "repository-list",
        "search-input",
        "agent-list",
        "agent-preview",
    ];
    const OWNERS: [&str; 3] = [
        "src/host_panel_models.rs",
        "src/workbench/panel_types.rs",
        "src/workbench/screens.rs",
    ];

    let source_root = repo_path("src");
    for path in rust_files(&source_root) {
        let relative = path
            .strip_prefix(repo_path(""))
            .unwrap_or_else(|error| panic!("strip repository prefix: {error}"))
            .to_string_lossy()
            .replace('\\', "/");
        if relative.contains("_test") || relative.contains("/tests/") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for panel_type in PRODUCT_TYPES {
            assert!(
                !source.contains(&format!("\"{panel_type}\""))
                    || OWNERS.contains(&relative.as_str()),
                "Dashboard product selector {panel_type:?} escaped its closed ownership boundary in {relative}"
            );
        }
    }
}

#[test]
fn generic_runtime_paths_do_not_branch_on_dashboard_identity() {
    const GENERIC_PATHS: [&str; 10] = [
        "src/action_context.rs",
        "src/action_projection.rs",
        "src/mouse_action_routing.rs",
        "src/mouse_routing.rs",
        "src/provider_panel_view.rs",
        "src/screen_layout.rs",
        "src/state/navigation_instance_ops.rs",
        "src/state/navigation_layers.rs",
        "src/state/navigation_ops.rs",
        "src/ui/components/provider_screen.rs",
    ];
    for path in GENERIC_PATHS {
        let source = read_repo(path);
        let production = source
            .split_once(
                "
#[cfg(test)]",
            )
            .map_or(source.as_str(), |(production, _)| production);
        assert!(!production.contains("DASHBOARD_IDENTITY"), "{path}");
        assert!(!production.contains("core.dashboard"), "{path}");
    }

    let navigation = read_repo("src/state/navigation.rs");
    assert_eq!(navigation.matches("DASHBOARD_IDENTITY").count(), 1);
    assert!(
        navigation.contains("#[cfg(test)]\nimpl Default for NavState"),
        "produce runtime paths must not branch on the Dashboard identity"
    );

    for path in rust_files(&repo_path("src/app_input")) {
        let relative = path.to_string_lossy();
        if relative.contains("_test") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(!source.contains("DASHBOARD_IDENTITY"), "{relative}");
        assert!(!source.contains("core.dashboard"), "{relative}");
    }
}

#[test]
fn host_factory_intent_precedes_private_model_state_adaptation() {
    let source = read_repo("src/state/host_panel_input_ops.rs");
    let reducer = source
        .split_once("pub fn apply_host_panel_action")
        .map(|(_, tail)| tail)
        .and_then(|tail| tail.split_once("pub fn scroll_host_panel"))
        .map_or_else(
            || panic!("host panel reducer boundaries changed"),
            |(reducer, _)| reducer,
        );
    let intent = reducer
        .find("control_intent_body")
        .unwrap_or_else(|| panic!("closed factory intent projection is absent"));
    for adaptation in ["apply_host_panel_event", "scroll_host_panel_kind"] {
        let position = reducer
            .find(adaptation)
            .unwrap_or_else(|| panic!("host state adaptation {adaptation} is absent"));
        assert!(
            intent < position,
            "private host state adaptation {adaptation} precedes factory intent"
        );
    }
}

#[test]
fn blocking_overlay_mouse_routing_precedes_every_underlay_path() {
    let source = read_repo("src/mouse_routing.rs");
    let router = source
        .split_once("pub fn handle_fullscreen_mouse")
        .map(|(_, tail)| tail)
        .and_then(|tail| tail.split_once("fn route_provider_panel_mouse"))
        .map_or_else(
            || panic!("fullscreen mouse router boundaries changed"),
            |(router, _)| router,
        );
    let blocking = router
        .find("route_blocking_overlay_mouse")
        .unwrap_or_else(|| panic!("blocking overlay route is absent"));
    for underlay in [
        "route_provider_panel_mouse",
        "route_terminal_gesture",
        "mouse_action_execution::try_up_click",
        "route_app_mouse_fallback",
    ] {
        let position = router
            .find(underlay)
            .unwrap_or_else(|| panic!("underlay route is absent: {underlay}"));
        assert!(blocking < position, "{underlay} precedes blocking overlays");
    }
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
    if evidence.schema != 1 || evidence.issue != 705 {
        errors.push("schema/issue identity differs".into());
    }
    let actual_criteria: Vec<&str> = evidence.criteria.keys().map(String::as_str).collect();
    if actual_criteria != CRITERIA {
        errors.push("criteria keys differ from CWR2-00..CWR2-11".into());
    }
    validate_exact_head(evidence, &mut errors);
    validate_artifacts(evidence, &mut errors);
    validate_criteria(evidence, &mut errors);
    validate_bidirectional_ownership(evidence, &mut errors);
    validate_symbols(evidence, &mut errors);
    validate_deletions(evidence, &mut errors);
    validate_residual_adapters(evidence, &mut errors);
    errors
}
fn validate_exact_head(evidence: &Evidence, errors: &mut Vec<String>) {
    if evidence.validation.phase != "post-S5" || evidence.validation.status != "GREEN" {
        errors.push("exact-head validation status differs from post-S5 GREEN".into());
    }
    if evidence.validation.base_revision != VALIDATED_BASE_REVISION {
        errors.push("validated base revision differs".into());
    }
    if evidence.validation.artifact_set_sha256 != artifact_set_sha256(&evidence.artifacts) {
        errors.push("validated artifact set hash differs".into());
    }
}

fn artifact_set_sha256(artifacts: &[Artifact]) -> String {
    let canonical = artifacts
        .iter()
        .fold(String::new(), |mut output, artifact| {
            let _ = writeln!(
                output,
                "{}\0{}\0{}\0{}",
                artifact.path,
                artifact.sha256,
                artifact.mode,
                artifact.criteria.join("\0")
            );
            output
        });
    Sha256::digest(canonical.as_bytes()).to_string()
}

fn validate_artifacts(evidence: &Evidence, errors: &mut Vec<String>) {
    let actual_paths: BTreeSet<&str> = evidence
        .artifacts
        .iter()
        .map(|artifact| artifact.path.as_str())
        .collect();
    let missing: Vec<&str> = REQUIRED_ARTIFACT_PATHS
        .into_iter()
        .filter(|path| !actual_paths.contains(path))
        .collect();
    if !missing.is_empty() {
        errors.push(format!(
            "required artifact ledger is incomplete: {missing:?}"
        ));
    }
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
    let approved_commands: BTreeSet<&str> = COMMANDS.into_iter().collect();
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
fn validate_bidirectional_ownership(evidence: &Evidence, errors: &mut Vec<String>) {
    let artifact_criteria: BTreeMap<&str, BTreeSet<&str>> = evidence
        .artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.path.as_str(),
                artifact.criteria.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    for (criterion_id, criterion) in &evidence.criteria {
        for path in &criterion.paths {
            if artifact_criteria
                .get(path.as_str())
                .is_some_and(|criteria| !criteria.contains(criterion_id.as_str()))
            {
                errors.push(format!(
                    "{criterion_id} path lacks matching artifact ownership: {path}"
                ));
            }
        }
    }
    for artifact in &evidence.artifacts {
        for criterion_id in &artifact.criteria {
            if evidence
                .criteria
                .get(criterion_id)
                .is_some_and(|criterion| !criterion.paths.contains(&artifact.path))
            {
                errors.push(format!(
                    "{} artifact ownership lacks matching {criterion_id} path",
                    artifact.path
                ));
            }
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
    let actual: Vec<(&str, &str)> = evidence
        .production_symbols
        .iter()
        .map(|owner| (owner.path.as_str(), owner.symbol.as_str()))
        .collect();
    if actual != REQUIRED_PRODUCTION_SYMBOLS {
        errors.push("production symbol ledger differs from canonical owners".into());
    }
    let artifact_criteria: BTreeMap<&str, BTreeSet<&str>> = evidence
        .artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.path.as_str(),
                artifact.criteria.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    for owner in &evidence.production_symbols {
        validate_criteria_list(&owner.criteria, &owner.symbol, errors);
        let source = read_repo(&owner.path);
        if !source.contains(&owner.symbol) {
            errors.push(format!(
                "production symbol is absent: {} in {}",
                owner.symbol, owner.path
            ));
        }
        let owner_criteria: BTreeSet<&str> = owner.criteria.iter().map(String::as_str).collect();
        if artifact_criteria
            .get(owner.path.as_str())
            .is_none_or(|criteria| !owner_criteria.is_subset(criteria))
        {
            errors.push(format!(
                "production owner lacks matching artifact ownership: {}",
                owner.path
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
    let deleted_paths: Vec<&str> = evidence
        .deleted_paths
        .iter()
        .map(|deletion| deletion.path.as_str())
        .collect();
    if deleted_paths != REQUIRED_DELETED_PATHS {
        errors.push("deleted path ledger differs from the exact migrated authorities".into());
    }
    let deleted_symbols: Vec<(&str, &str)> = evidence
        .deleted_symbols
        .iter()
        .map(|deletion| (deletion.scope.as_str(), deletion.symbol.as_str()))
        .collect();
    if deleted_symbols != REQUIRED_DELETED_SYMBOLS {
        errors.push("deleted symbol ledger differs from the exact migrated authorities".into());
    }
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

fn validate_residual_adapters(evidence: &Evidence, errors: &mut Vec<String>) {
    let actual: Vec<(&str, &str)> = evidence
        .residual_adapters
        .iter()
        .map(|adapter| (adapter.screen.as_str(), adapter.path.as_str()))
        .collect();
    if actual != REQUIRED_RESIDUAL_ADAPTERS {
        errors.push("residual adapter ledger differs from the exact residual seven".into());
    }
    for adapter in &evidence.residual_adapters {
        validate_criteria_list(&adapter.criteria, &adapter.screen, errors);
        if adapter.criteria != ["CWR2-09", "CWR2-11"] {
            errors.push(format!(
                "residual adapter {} has incomplete criterion ownership",
                adapter.screen
            ));
        }
        let source = read_repo(&adapter.path);
        if !source.contains(&format!("ScreenId::{}", adapter.screen)) {
            errors.push(format!(
                "residual adapter is absent: {} in {}",
                adapter.screen, adapter.path
            ));
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
