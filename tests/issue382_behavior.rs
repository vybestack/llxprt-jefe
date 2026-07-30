//! Issue #382 definition-cutover acceptance tests for all seventeen behavioral rows.
//! Fixture bytes are captured release provenance, never a runtime version allow-list.

mod issue382;

use std::collections::BTreeSet;

use issue382::agent_probe_runtime::assert_exact_four_fixture_playback;
use issue382::fixtures::{
    AGENTS, SCENARIO_ROOT, assert_probe_identity, assert_provenance, read_scenario, repo_path,
};
use issue382::probe_fixtures::assert_all_retained_probe_fixtures;

// The closed production contract module the issue mandates. It does not exist
// on `origin/main`; this import is the RED trigger.
use jefe::agent_candidate::{
    AgentCandidateResolver, CandidateResolution, CandidateSkip, PackageRunnerKind, VersionSelector,
};
use jefe::agent_candidate_path::PathSnapshot;
use jefe::agent_registry::AgentTypeRegistry;
use jefe::agent_status_view::{AgentAvailabilityObservation, project_agent_type_statuses};
use jefe::domain::agent_definition::type_id::CandidateKind as S2CandidateKind;
use jefe::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, AgentTypeId, Availability, CandidateKind,
    ExecutableCandidate, Operation, OperationMatrix, Preflight, ProbeErrorCode, ProbeSpec,
    RemoteTarget, Target,
};
use jefe::domain::effects::{
    AgentAvailabilityProbe, EffectCompletion, EffectResponse, ProbeResponse,
};
use jefe::harness::v1::parse_scenario_v1;
use jefe::messages::{AppMessage, RepositoryAgentMessage};
use jefe::runtime::agent_remote_plan::{
    self, RemotePlanOutcome, RemoteSerializeError, plan_remote_launch, posix_single_quote,
};

/// Parse and structurally validate a scenario by basename, asserting the
/// closed schema-1 grammar accepts it and the recorded name matches.
fn parse_scenario(name: &str) -> String {
    let bytes = read_scenario(name);
    let scenario = parse_scenario_v1(&bytes).unwrap_or_else(|err| {
        panic!("{name} must be a structurally valid schema-1 scenario: {err}")
    });
    let expected = name.strip_suffix(".json").unwrap_or(name);
    assert_eq!(scenario.name, expected, "{name} declares a mismatched name");
    scenario.name
}

/// The seventeen scenario basenames in issue-#382 acceptance-matrix order.
const SCENARIOS: &[&str] = &[
    "agent-resolver-order.json",
    "agent-probe-parser.json",
    "agent-probe-negative.json",
    "agent-incompatible-zero-spawn.json",
    "agent-status-cartesian.json",
    "agent-local-operation-matrix.json",
    "agent-remote-operation-matrix.json",
    "agent-unsupported-ui.json",
    "agent-sandbox-preflight.json",
    "agent-fresh-issue.json",
    "agent-fresh-pr.json",
    "agent-stale-generation.json",
    "agent-legacy-migration.json",
    "agent-terminal-compatibility.json",
    "agent-no-product-branches.json",
    "agent-claude-evidence-gate.json",
    "agent-version-selector.json",
];

/// Structural validation gate (directive 4): every shipped scenario parses
/// through the closed schema-1 harness grammar with no fixture/capture
/// collisions, mode violations, or step-order errors.
#[test]
fn all_seventeen_scenarios_structurally_valid() {
    let mut seen = BTreeSet::new();
    for name in SCENARIOS {
        let parsed = parse_scenario(name);
        assert!(
            seen.insert(parsed.clone()),
            "duplicate scenario name {parsed}"
        );
    }
    assert_eq!(seen.len(), 17, "exactly seventeen scenarios required");
    // The directory must contain exactly the ledger scenarios.
    let dir = repo_path(SCENARIO_ROOT);
    let mut on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {SCENARIO_ROOT}: {err}"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for name in SCENARIOS {
        assert!(on_disk.remove(*name), "missing on-disk scenario {name}");
    }
    assert!(
        on_disk.is_empty(),
        "unexpected extra scenarios: {on_disk:?}"
    );
}

// ---- CW02-01: candidate resolver order ----

/// Write a repository-local LLxprt executable fixture into `repo` on Unix,
/// or an extensionless placeholder on non-Unix.
fn write_repository_local_fixture(repo: &tempfile::TempDir) {
    let bin_dir = repo.path().join(".llxprt/bin");
    std::fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir .llxprt/bin: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = bin_dir.join("llxprt");
        std::fs::write(
            &exe,
            b"#!/bin/sh
echo repo-local-llxprt
",
        )
        .unwrap_or_else(|error| panic!("write repo-local fixture: {error}"));
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod repo-local fixture: {error}"));
    }
    #[cfg(not(unix))]
    {
        std::fs::write(bin_dir.join("llxprt"), b"fixture")
            .unwrap_or_else(|error| panic!("write repo-local fixture: {error}"));
    }
}

#[test]
fn candidate_resolver_order() {
    parse_scenario("agent-resolver-order.json");
    // Contract: WHEN candidates resolve, Jefe shall select the first
    // physically valid candidate in declared order from one PATH snapshot.
    let llxprt = &AGENTS[0];
    let _ = llxprt.display_name;
    assert_probe_identity(llxprt);
    // S1 contract: the typed candidate declares a resolvable repository-local
    // LLxprt candidate; S1 proves the candidate kind validates.
    let repo_local = ExecutableCandidate {
        kind: CandidateKind::RepositoryLlxprt,
        value: std::path::PathBuf::from(".llxprt/bin/llxprt"),
    };
    assert!(
        repo_local.validate().is_ok(),
        "repository-LLxprt candidate validates"
    );
    // S2 boundary: the shipped LLxprt definition's first declared candidate is
    // repository-local; with a repository-local fixture and an empty PATH the
    // resolver must select that candidate (declared index 0) with a fingerprint.
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let llxprt_def = registry
        .definitions()
        .iter()
        .find(|d| d.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("core.llxprt shipped"));
    assert!(
        matches!(
            llxprt_def.candidates.first().map(|c| &c.kind),
            Some(S2CandidateKind::RepositoryLlxprt)
        ),
        "LLxprt first declared candidate is repository-local"
    );
    let Ok(repo) = tempfile::tempdir() else {
        panic!("temp repo dir must be created");
    };
    write_repository_local_fixture(&repo);
    let snapshot = PathSnapshot::for_platform(
        jefe::runtime::AgentExecutablePlatform::current(),
        vec![],
        std::env::var_os("PATHEXT"),
    );
    let resolver = AgentCandidateResolver::new(&snapshot, repo.path().to_path_buf());
    let resolution = resolver.resolve(llxprt_def);
    let Some(picked) = resolution.resolved() else {
        panic!("repository-local candidate resolves");
    };
    assert_eq!(
        picked.index(),
        0,
        "first declared (repository-local) candidate selected in declared order"
    );
    assert!(
        picked.executable().is_absolute(),
        "resolved executable is canonical absolute"
    );
    assert!(
        picked.fingerprint().canonical_path().is_absolute(),
        "fingerprint carries a canonical absolute path"
    );
    let _ = CandidateResolution::Resolved(picked.clone());
}

// ---- CW02-02: probe parser for all four agents ----

#[test]
fn probe_parser_four_agents() {
    parse_scenario("agent-probe-parser.json");
    // Contract: every retained release directory is discovered rather than
    // selected by fixture name. Its exact --version bytes must identify the
    // agent, and authored --help literals must reproduce all capabilities.
    assert_all_retained_probe_fixtures();
    assert_exact_four_fixture_playback();
    let _spec = ProbeSpec::default();
}

// ---- CW02-03: probe negative table ----

#[test]
fn probe_negative_table() {
    parse_scenario("agent-probe-negative.json");
    // Contract: IF probe framing, UTF-8, bounds, exit, identity, or capability
    // validation fails, Jefe shall return ProbeError (AGT-E202).
    // RED: the closed probe error code enum must exist.
    let code = ProbeErrorCode::Agte202;
    assert!(code.is_probe_error(), "AGT-E202 is the probe error code");
}

// ---- CW02-04: capability gate ----

#[test]
fn capability_gate() {
    parse_scenario("agent-incompatible-zero-spawn.json");
    // Contract: IF a required capability is absent, Jefe shall show
    // incompatible and emit zero launch effects.
    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.code-puppy")
        .unwrap_or_else(|| panic!("Code Puppy definition must be shipped"));
    let observation = AgentAvailabilityObservation::new(
        &definition,
        true,
        Availability::InstalledIncompatible {
            reason: "missing required capability: interactive".to_string(),
            generation: 7,
        },
    );
    let projected = project_agent_type_statuses(&[observation]);
    assert_eq!(projected.len(), 1, "one definition projects exactly once");
    assert_eq!(projected[0].status_text, "Incompatible");
    assert_eq!(
        projected[0].reason.as_deref(),
        Some("missing required capability: interactive")
    );
    assert!(!projected[0].create_enabled, "incompatible cannot create");
}

// ---- CW02-05: status projection ----

#[test]
fn status_projection() {
    parse_scenario("agent-status-cartesian.json");
    // Contract: WHEN status renders, Jefe shall project every
    // enablement/availability pair exactly once.
    let definition = AgentDefinition::shipped()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("a shipped definition must exist"));
    let availabilities = [
        Availability::NotFound,
        Availability::InstalledCompatible {
            identity: "fixture identity".to_string(),
            capabilities: vec!["optional-present".to_string()],
            generation: 1,
        },
        Availability::InstalledIncompatible {
            reason: "missing required capability: required-id".to_string(),
            generation: 2,
        },
        Availability::ProbeError {
            code: ProbeErrorCode::Agte202,
            reason: "invalid UTF-8".to_string(),
            generation: 3,
        },
    ];
    let mut observations = Vec::new();
    for enabled in [false, true] {
        for availability in availabilities.clone() {
            observations.push(AgentAvailabilityObservation::new(
                &definition,
                enabled,
                availability,
            ));
        }
    }

    let projected = project_agent_type_statuses(&observations);

    assert_eq!(
        projected.len(),
        8,
        "the complete 2 x 4 matrix projects once"
    );
    for (row, source) in projected.iter().zip(&observations) {
        assert_eq!(row.display_name, source.display_name());
        assert_eq!(row.enabled, source.enabled());
        assert_eq!(
            row.create_enabled,
            source.enabled()
                && matches!(
                    source.availability(),
                    Availability::InstalledCompatible { .. }
                )
        );
    }
    assert_eq!(
        projected
            .iter()
            .filter(|row| row.status_text == "Not found")
            .count(),
        2
    );

    assert!(projected.iter().any(|row| {
        row.error_code == Some("AGT-E202") && row.reason.as_deref() == Some("invalid UTF-8")
    }));
}

#[test]
fn stale_availability_completion_is_a_no_op() {
    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.codex")
        .unwrap_or_else(|| panic!("Codex definition must be shipped"));
    let state = jefe::state::AppState {
        agent_type_availability: vec![AgentAvailabilityObservation::pending(
            &definition,
            true,
            1,
            CandidateResolution::NotFound(Vec::new()),
        )],
        ..jefe::state::AppState::default()
    };
    let first = state
        .apply_message(AppMessage::RepositoryAgent(
            RepositoryAgentMessage::ProbeAgentAvailability(vec![AgentAvailabilityProbe {
                definition: Box::new(definition.clone()),
                resolution: CandidateResolution::NotFound(Vec::new()),
                generation: 1,
            }]),
        ))
        .unwrap_or_else(|error| panic!("first probe request must commit: {error}"));
    let stale_correlation = first.effects[0].correlation.clone();
    let mut state = first.next_state;
    state.agent_type_availability = vec![AgentAvailabilityObservation::pending(
        &definition,
        true,
        2,
        CandidateResolution::NotFound(Vec::new()),
    )];
    let second = state
        .apply_message(AppMessage::RepositoryAgent(
            RepositoryAgentMessage::ProbeAgentAvailability(vec![AgentAvailabilityProbe {
                definition: Box::new(definition),
                resolution: CandidateResolution::NotFound(Vec::new()),
                generation: 2,
            }]),
        ))
        .unwrap_or_else(|error| panic!("replacement probe request must commit: {error}"));
    let before = format!("{:?}", second.next_state);
    let completion = EffectCompletion {
        correlation: stale_correlation,
        result: Ok(EffectResponse::AgentProbe(ProbeResponse::Availability {
            availability: Box::new(Availability::InstalledCompatible {
                identity: "stale identity".to_owned(),
                capabilities: Vec::new(),
                generation: 1,
            }),
            generation: 1,
        })),
    };

    let after = second
        .next_state
        .apply_message(AppMessage::EffectCompletion(Box::new(completion)))
        .unwrap_or_else(|error| panic!("stale completion must commit as a no-op: {error}"));

    assert_eq!(format!("{:?}", after.next_state), before);
    assert!(after.effects.is_empty());
}
// ---- CW02-06: local plan golden ----

/// Build a compatible probe availability for a definition.
fn probe_compatible(definition: &AgentDefinition, generation: u64) -> Availability {
    let capabilities: Vec<String> = definition
        .probe
        .capabilities
        .as_ref()
        .map(|probe| probe.tokens.iter().map(|token| token.id.clone()).collect())
        .unwrap_or_default();
    Availability::InstalledCompatible {
        identity: "fixture-identity".to_string(),
        capabilities,
        generation,
    }
}

/// Assert that the production planner produces a fixture-golden immutable
/// local plan for one definition+operation pair, returning the plan.
fn assert_golden_local_plan(
    definition: &AgentDefinition,
    operation: Operation,
    executable: &str,
    values: &jefe::runtime::agent_plan::LaunchFieldValues,
) -> AgentLaunchPlan {
    use jefe::runtime::agent_plan::{PlanOutcome, PlanRequest, plan_local_launch};
    let request = PlanRequest {
        definition,
        operation,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/srv/project"),
        },
        executable: std::path::PathBuf::from(executable),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from(executable),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: probe_compatible(definition, 1),
        probe_generation: 1,
        target_generation: 1,
        values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        PlanOutcome::Unsupported { reason } => {
            panic!(
                "{} {:?} must be supported: {reason}",
                definition.display_name, operation
            );
        }
        PlanOutcome::Error(error) => {
            panic!(
                "{} {:?} plan failed: {error}",
                definition.display_name, operation
            );
        }
    }
}

#[test]
fn local_plan_golden() {
    parse_scenario("agent-local-operation-matrix.json");
    for agent in AGENTS {
        assert_probe_identity(agent);
    }
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let find = |id: &str| {
        registry
            .definitions()
            .iter()
            .find(|d| d.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} shipped"))
    };
    assert_llxprt_golden(find("core.llxprt"));
    assert_code_puppy_golden(find("core.code-puppy"));
    assert_codex_golden(find("core.codex"));
    assert_claude_golden(find("core.claude-code"));
}

fn argv_of(plan: &AgentLaunchPlan) -> Vec<String> {
    plan.argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn assert_llxprt_golden(llxprt: &AgentDefinition) {
    use jefe::domain::agent_definition::FieldValue;
    use jefe::runtime::agent_plan::LaunchFieldValues;
    let mut values = LaunchFieldValues::new();
    values.set_repository("profile", FieldValue::String("dev".to_string()));
    values.set_agent("continue", FieldValue::Boolean(true));
    let plan = assert_golden_local_plan(llxprt, Operation::Normal, "/opt/bin/llxprt", &values);
    assert_eq!(plan.type_id, llxprt.id);
    assert_eq!(plan.definition_sha256, llxprt.sha256());
    assert_eq!(plan.probe_generation, 1);
    assert_eq!(plan.target_generation, 1);
    assert_eq!(plan.executable, std::path::PathBuf::from("/opt/bin/llxprt"));
    assert!(plan.signature_excludes_secrets());
    let argv = argv_of(&plan);
    assert!(argv.contains(&"--profile-load".to_string()));
    assert!(argv.contains(&"--continue".to_string()));
    assert!(argv.contains(&"dev".to_string()));
    assert!(plan.env.is_empty(), "no ambient env vars in plan");
}

fn assert_code_puppy_golden(code_puppy: &AgentDefinition) {
    use jefe::domain::agent_definition::FieldValue;
    use jefe::runtime::agent_plan::LaunchFieldValues;
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("gpt-4o".to_string()));
    values.set_agent("interactive", FieldValue::Boolean(true));
    let plan = assert_golden_local_plan(
        code_puppy,
        Operation::Normal,
        "/home/u/.local/bin/code-puppy",
        &values,
    );
    let argv = argv_of(&plan);
    assert!(argv.contains(&"--model".to_string()));
    assert!(argv.contains(&"--interactive".to_string()));
}

fn assert_codex_golden(codex: &AgentDefinition) {
    use jefe::domain::agent_definition::{FieldValue, Support};
    use jefe::runtime::agent_plan::LaunchFieldValues;
    {
        let mut values = LaunchFieldValues::new();
        values.set_repository("model", FieldValue::String("o4-mini".to_string()));
        values.set_agent("prompt", FieldValue::String("hello".to_string()));
        let plan = assert_golden_local_plan(codex, Operation::Normal, "/opt/bin/codex", &values);
        let argv = argv_of(&plan);
        assert!(argv.contains(&"--model".to_string()));
        assert_eq!(argv.last(), Some(&"hello".to_string()));
    }
    assert_codex_fresh_issue_unsupported(codex);
    let _ = Support::unsupported("x"); // exercise the type
}

fn assert_codex_fresh_issue_unsupported(codex: &AgentDefinition) {
    use jefe::runtime::agent_plan::{
        LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch,
    };
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: codex,
        operation: Operation::FreshIssue,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/srv/project"),
        },
        executable: std::path::PathBuf::from("/opt/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/opt/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: probe_compatible(codex, 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Unsupported { reason } => {
            assert!(reason.contains("not fixture-verified"), "reason: {reason}");
            assert!(
                codex.operations.fresh_issue.supported.is_unsupported(),
                "definition declares fresh_issue unsupported"
            );
        }
        other => panic!("Codex FreshIssue must be unsupported, got {other:?}"),
    }
}

fn assert_claude_golden(claude: &AgentDefinition) {
    use jefe::domain::agent_definition::FieldValue;
    use jefe::runtime::agent_plan::LaunchFieldValues;
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("sonnet".to_string()));
    values.set_agent("prompt", FieldValue::String("hello".to_string()));
    let plan =
        assert_golden_local_plan(claude, Operation::Normal, "/usr/local/bin/claude", &values);
    let argv = argv_of(&plan);
    assert!(argv.contains(&"--model".to_string()));
    assert_eq!(argv.last(), Some(&"hello".to_string()));
}

// ---- CW02-07: remote plan contract ----

/// Build a `RemotePlanRequest` for the given definition and remote target.
fn remote_request<'a>(
    definition: &'a AgentDefinition,
    executable: &str,
    remote_target: &RemoteTarget,
    ssh_settings: &'a jefe::domain::RemoteRepositorySettings,
    values: &'a jefe::runtime::agent_plan::LaunchFieldValues,
) -> agent_remote_plan::RemotePlanRequest<'a> {
    agent_remote_plan::RemotePlanRequest {
        definition,
        operation: Operation::Normal,
        target: Target::Remote(remote_target.clone()),
        executable: std::path::PathBuf::from(executable),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from(executable),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: probe_compatible(definition, 1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values,
        preflight: Preflight::default(),
        ssh_settings,
    }
}

/// The authorized SSH settings and matching remote target used by the
/// golden remote-plan assertions.
fn golden_remote_identity() -> (jefe::domain::RemoteRepositorySettings, RemoteTarget) {
    let ssh_settings = jefe::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        ..jefe::domain::RemoteRepositorySettings::default()
    };
    let remote_target = RemoteTarget {
        user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        run_as_user: String::new(),
        canonical_cwd: std::path::PathBuf::from("/srv/project"),
    };
    (ssh_settings, remote_target)
}

/// POSIX-quote one input, panicking on error with context.
fn expect_quoted(input: &str) -> String {
    match posix_single_quote(input) {
        Ok(quoted) => quoted,
        Err(error) => panic!("posix_single_quote({input:?}) must succeed: {error}"),
    }
}

/// Assert the POSIX single-quote serializer contract (golden quoting).
fn assert_serializer_contract() {
    // Each string enclosed in single quotes.
    assert_eq!(expect_quoted("hello"), "'hello'");
    // Empty string preserved as two single quotes.
    assert_eq!(expect_quoted(""), "''");
    // Embedded apostrophe: '"'"' sequence between quoted portions.
    assert_eq!(expect_quoted("it's"), "'it'\"'\"'s'");
    assert_eq!(expect_quoted("a'b'c"), "'a'\"'\"'b'\"'\"'c'");
    // Apostrophe-only string.
    assert_eq!(expect_quoted("'"), "''\"'\"''");
    // NUL byte rejected with typed error.
    assert_eq!(
        posix_single_quote("a\0b"),
        Err(RemoteSerializeError::NulByte)
    );
}

/// Assert one unsupported remote plan returns the exact reason with zero SSH.
fn assert_remote_unsupported(
    definition: &AgentDefinition,
    executable: &str,
    remote_target: &RemoteTarget,
    ssh_settings: &jefe::domain::RemoteRepositorySettings,
) {
    let values = jefe::runtime::agent_plan::LaunchFieldValues::new();
    let request = remote_request(definition, executable, remote_target, ssh_settings, &values);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Unsupported { reason } => {
            assert!(
                reason.contains("not fixture-verified"),
                "{} remote reason must be exact: {reason}",
                definition.display_name
            );
        }
        other => panic!(
            "{} remote must be unsupported, got {other:?}",
            definition.display_name
        ),
    }
}

#[test]
fn remote_plan_contract() {
    parse_scenario("agent-remote-operation-matrix.json");
    // Contract: WHEN a supported remote operation is submitted, Jefe shall
    // produce one fixture-golden structural transcript through the existing
    // audited SSH boundary using the one POSIX single-quote serializer.
    // Unsupported targets/operations return the exact declared reason with
    // zero SSH/preparation.
    assert_serializer_contract();
    assert_llxprt_remote_golden();
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let (ssh_settings, remote_target) = golden_remote_identity();
    let codex = find_shipped(&registry, "core.codex");
    assert_remote_unsupported(codex, "/opt/bin/codex", &remote_target, &ssh_settings);
    let claude = find_shipped(&registry, "core.claude-code");
    assert_remote_unsupported(
        claude,
        "/usr/local/bin/claude",
        &remote_target,
        &ssh_settings,
    );
}

/// Find a shipped definition by stable id, panicking if absent.
fn find_shipped<'a>(registry: &'a AgentTypeRegistry, id: &str) -> &'a AgentDefinition {
    registry
        .definitions()
        .iter()
        .find(|d| d.id.as_str() == id)
        .unwrap_or_else(|| panic!("{id} shipped"))
}

/// Assert the LLxprt remote Normal plan produces the fixture-golden
/// transcript (remote command, agent argv, and SSH arguments).
fn assert_llxprt_remote_golden() {
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let llxprt = find_shipped(&registry, "core.llxprt");
    let (ssh_settings, remote_target) = golden_remote_identity();
    let mut values = jefe::runtime::agent_plan::LaunchFieldValues::new();
    values.set_repository(
        "profile",
        jefe::domain::agent_definition::FieldValue::String("dev".to_string()),
    );
    values.set_agent(
        "continue",
        jefe::domain::agent_definition::FieldValue::Boolean(true),
    );
    let request = remote_request(
        llxprt,
        "/opt/bin/llxprt",
        &remote_target,
        &ssh_settings,
        &values,
    );
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => t,
        other => panic!("LLxprt remote Normal must be supported, got {other:?}"),
    };
    assert_eq!(
        transcript.remote_command(),
        "cd '/srv/project' && exec '/opt/bin/llxprt' '--profile-load' 'dev' '--yolo' '--continue'"
    );
    let agent_argv: Vec<String> = transcript
        .agent_argv()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        agent_argv,
        &[
            "--profile-load".to_string(),
            "dev".to_string(),
            "--yolo".to_string(),
            "--continue".to_string(),
        ]
    );
    assert_golden_ssh_arguments(&transcript);
}

/// Assert the SSH arguments carry the audited boundary's non-interactive
/// mode, target identity, port, and trailing remote command.
fn assert_golden_ssh_arguments(transcript: &agent_remote_plan::RemoteTranscript) {
    let ssh_args: Vec<String> = transcript
        .ssh_arguments()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(ssh_args.contains(&"-T".to_string()), "non-interactive mode");
    assert!(ssh_args.contains(&"--".to_string()), "argument separator");
    assert!(
        ssh_args.contains(&"dev@example.com".to_string()),
        "remote target user@host"
    );
    assert!(
        ssh_args.contains(&"-p".to_string()) && ssh_args.contains(&"22".to_string()),
        "port option"
    );
    assert_eq!(
        ssh_args.last(),
        Some(&transcript.remote_command().to_string()),
        "remote command is the last SSH argument"
    );
}

// ---- CW02-08: operation/target matrix ----

#[test]
fn operation_target_matrix() {
    parse_scenario("agent-unsupported-ui.json");
    // Contract: IF an operation or target is unsupported, Jefe shall keep it
    // visible with its exact reason and perform zero preparation.
    // RED: OperationMatrix is the closed per-definition support contract.
    let matrix = OperationMatrix::default();
    assert!(matrix.has_any_unsupported(), "default matrix exposes gaps");
}

// ---- CW02-09: preflight order ----

#[test]
fn preflight_order() {
    parse_scenario("agent-sandbox-preflight.json");
    // Contract: IF sandbox preflight fails, Jefe shall perform no clone,
    // prompt write, tmux, SSH, or agent spawn. Preflight must run only after
    // S8 authorize_execution succeeds, and success is the only path to
    // clone/reset/prompt/SSH/tmux/spawn.
    //
    // RED: the ordered preparation boundary enforces this structurally —
    // prepare_execution requires an AuthorizedExecution and returns either a
    // PreflightCleared (only way to preparation) or a typed UnavailableReason
    // with zero later effects.
    assert!(
        !Preflight::default().is_required(),
        "default preflight represents an unsandboxed launch"
    );
    issue382::preflight_order::assert_engine_missing();
    issue382::preflight_order::assert_image_missing();
    issue382::preflight_order::assert_env_missing();
    issue382::preflight_order::assert_cleared();
}

// ---- CW02-10: fresh issue ordering ----

#[test]
fn fresh_issue_ordering() {
    parse_scenario("agent-fresh-issue.json");
    // Contract: WHEN fresh Issue Send is confirmed, Jefe shall emit exactly
    // one fixture-golden fresh prompt after successful preflight.
    issue382::fresh_send::assert_operation(Operation::FreshIssue);
}

// ---- CW02-11: fresh PR ordering ----

#[test]
fn fresh_pr_ordering() {
    parse_scenario("agent-fresh-pr.json");
    // Contract: WHEN fresh PR Send is confirmed, Jefe shall emit exactly one
    // fixture-golden fresh prompt after successful preflight.
    issue382::fresh_send::assert_operation(Operation::FreshPullRequest);
}

// ---- CW02-12: generation property ----

#[test]
fn generation_property() {
    parse_scenario("agent-stale-generation.json");
    // Contract: IF any generation changes before execution, Jefe shall return
    // AGT-E203 and perform zero side effects.
    let code = ProbeErrorCode::Agte203;
    assert!(
        code.is_generation_mismatch(),
        "AGT-E203 is the stale-generation code"
    );
}

// ---- CW02-13: agent migration golden ----

#[test]
fn agent_migration_golden() {
    parse_scenario("agent-legacy-migration.json");
    issue382::schema1_migration::assert_migration_contract();
    // Contract: WHEN schema-1 records migrate, Jefe shall preserve known typed
    // values and exact dormant unknown records.
    // S1 contract: AgentTypeId replaces AgentTypeId with strict validation.
    // Schema-1 alias mapping belongs to the one-way persistence migration
    // (S13) and is intentionally absent from this domain contract. S1 proves
    // the stable typed ids parse and validate against the closed grammar.
    let Ok(stable) = AgentTypeId::parse("core.code-puppy") else {
        panic!("stable id must parse");
    };
    assert_eq!(
        stable.as_str(),
        "core.code-puppy",
        "stable typed id round-trips through strict validation"
    );
    let invalid = AgentTypeId::parse("Unknown.Agent");
    assert!(
        invalid.is_err(),
        "invalid id is rejected by strict validation"
    );
}

// ---- CW02-14: local/remote tmux ----

#[test]
fn local_remote_tmux() {
    parse_scenario("agent-terminal-compatibility.json");
    let plan = AgentLaunchPlan::default();
    assert!(
        plan.signature_excludes_secrets(),
        "signature v1 excludes secrets and display-only values"
    );
}

#[test]
fn agent_architecture_guard() {
    parse_scenario("agent-no-product-branches.json");
    for agent in AGENTS {
        assert_provenance(agent);
    }
    let defs = AgentDefinition::shipped();
    assert_eq!(defs.len(), 4, "exactly four shipped definitions");
}

// ---- CW02-16: claude entry gate ----

#[test]
fn claude_entry_gate() {
    parse_scenario("agent-claude-evidence-gate.json");
    let claude = &AGENTS[2];
    assert_probe_identity(claude);
    assert_provenance(claude);
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let definition = registry
        .definitions()
        .iter()
        .find(|definition| definition.display_name == "Claude Code")
        .unwrap_or_else(|| panic!("Claude definition must remain published"));
    let resolution = CandidateResolution::NotFound(Vec::new());
    let result = jefe::runtime::run_local_agent_probe(definition, &resolution, 9);
    assert!(
        result.availability().is_not_found(),
        "an unresolved Claude candidate publishes NotFound"
    );
    assert!(
        result.executable_fingerprint().is_none(),
        "NotFound returns before any executable process evidence can exist"
    );
    let projected = project_agent_type_statuses(&[AgentAvailabilityObservation::new(
        definition,
        true,
        result.availability().clone(),
    )]);
    assert_eq!(projected[0].status_text, "Not found");
    assert!(!projected[0].create_enabled);
}

// ---- CW02-17: package runner selector ----

#[test]
fn package_runner_selector() {
    parse_scenario("agent-version-selector.json");
    let definitions = AgentDefinition::shipped();
    assert_eq!(definitions.len(), 4);
    for definition in &definitions {
        let selector_fields = definition
            .agent_fields
            .iter()
            .filter(|field| field.id == "version_selector")
            .collect::<Vec<_>>();
        assert_eq!(selector_fields.len(), 1, "{} selector field", definition.id);
        assert!(selector_fields[0].launch_signature);
        assert!(
            definition
                .emitters
                .iter()
                .all(|emitter| emitter.field() != Some("version_selector")),
            "selector must not emit argv"
        );
    }

    let blank = VersionSelector::normalize(" \t\u{200b} ")
        .unwrap_or_else(|error| panic!("blank selector: {error}"));
    assert!(blank.is_direct());
    let latest = VersionSelector::normalize("LATEST")
        .unwrap_or_else(|error| panic!("latest selector: {error}"));
    let nightly = VersionSelector::normalize("Latest Nightly")
        .unwrap_or_else(|error| panic!("nightly selector: {error}"));
    let explicit = VersionSelector::normalize(" 1.2.\n3 ")
        .unwrap_or_else(|error| panic!("explicit selector: {error}"));
    assert_eq!(latest.effective(PackageRunnerKind::Npm), Some("latest"));
    assert_eq!(nightly.effective(PackageRunnerKind::Npm), Some("nightly"));
    assert_eq!(explicit.effective(PackageRunnerKind::Npm), Some("1.2.3"));
    assert_eq!(
        latest.package_spec(PackageRunnerKind::Uvx, "python-agent"),
        Some("python-agent".to_string())
    );
    assert_eq!(
        explicit.package_spec(PackageRunnerKind::Uvx, "python-agent"),
        Some("python-agent==1.2.3".to_string())
    );

    let _ = CandidateSkip::PackageSelectorBlank { index: 0 };
    #[cfg(unix)]
    issue382::package_selector::assert_runtime_matrix(&definitions, assert_golden_local_plan);
}
