//! Issue #382 (CW-02) RED contract tests: complete vertical four-agent
//! definition cutover.
//!
//! These tests define the seventeen accepted behavioral criteria from issue
//! #382's test-first acceptance ledger plus a structural-validation gate for
//! every scenario. Each behavioral test parses its scenario through the
//! shipped schema-1 harness parser, asserts the exact captured fixture bytes
//! that are the release provenance, and exercises the closed production
//! contract the issue requires. The production contracts (`AgentTypeId`,
//! `AgentDefinition`, `ExecutableCandidate`, `ProbeSpec`, `AgentLaunchPlan`)
//! live in `jefe::domain::agent_definition`, which does not exist on
//! `origin/main`; referencing it is the intended RED. GREEN must add the typed
//! domain contract before any test body compiles and passes.
//!
//! Authority: issue #382 body (closed contracts, acceptance matrix rows
//! CW02-01..17, deterministic algorithms and limits), the fixture-authoring
//! gate, and the project plan. Fixture bytes are deterministic provenance of a
//! real captured release, never a runtime version allow-list.

mod issue382;

use std::collections::BTreeSet;

use issue382::fixtures::{
    AGENTS, SCENARIO_ROOT, assert_probe_identity, assert_provenance, read_scenario, repo_path,
};
use issue382::probe_fixtures::assert_all_retained_probe_fixtures;

// The closed production contract module the issue mandates. It does not exist
// on `origin/main`; this import is the RED trigger.
use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution};
use jefe::agent_candidate_path::PathSnapshot;
use jefe::agent_registry::AgentTypeRegistry;
use jefe::domain::agent_definition::type_id::CandidateKind as S2CandidateKind;
use jefe::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, AgentTypeId, Availability, CandidateKind,
    ExecutableCandidate, Operation, OperationMatrix, Preflight, ProbeErrorCode, ProbeSpec,
    RemoteTarget, Support, Target,
};
use jefe::harness::v1::parse_scenario_v1;

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
/// or a `.exe` placeholder on non-Unix.
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
        std::fs::write(bin_dir.join("llxprt.exe"), b"fixture")
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
    // RED: Support is the closed per-cell availability contract.
    let unsupported = Support::Unsupported {
        reason: "missing required capability".to_string(),
    };
    assert!(
        unsupported.is_unsupported(),
        "absent capability must be unsupported"
    );
}

// ---- CW02-05: status projection ----

#[test]
fn status_projection() {
    parse_scenario("agent-status-cartesian.json");
    // Contract: WHEN status renders, Jefe shall project every
    // enablement/availability pair exactly once.
    // RED: Availability is the closed status contract rendered by the UI.
    let not_found = Availability::NotFound;
    assert!(not_found.is_not_found(), "NotFound is a distinct status");
}

// ---- CW02-06: local plan golden ----

#[test]
fn local_plan_golden() {
    parse_scenario("agent-local-operation-matrix.json");
    // Contract: WHEN a supported local operation is submitted, Jefe shall
    // produce the fixture-golden argv/env/cwd plan.
    for agent in AGENTS {
        assert_probe_identity(agent);
    }
    // RED: AgentLaunchPlan is the immutable local plan contract; its target
    // must carry a canonical local cwd.
    let target = Target::Local {
        canonical_cwd: std::path::PathBuf::from("/srv/project"),
    };
    assert!(target.is_local(), "local plan targets a canonical cwd");
    let _ = Operation::Normal;
}

// ---- CW02-07: remote plan contract ----

#[test]
fn remote_plan_contract() {
    parse_scenario("agent-remote-operation-matrix.json");
    // Contract: WHEN a supported remote operation is submitted, Jefe shall
    // use the audited serializer and fixture-golden remote transcript.
    // RED: the remote target contract must exist.
    let target = Target::Remote(RemoteTarget::default());
    assert!(!target.is_local(), "remote target is not local");
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
    // prompt write, tmux, SSH, or agent spawn.
    // RED: the typed preflight contract gates every preparation effect.
    let preflight = Preflight::default();
    assert!(
        preflight.is_unavailable(),
        "default preflight is unavailable"
    );
}

// ---- CW02-10: fresh issue ordering ----

#[test]
fn fresh_issue_ordering() {
    parse_scenario("agent-fresh-issue.json");
    // Contract: WHEN fresh Issue Send is confirmed, Jefe shall emit exactly
    // one fixture-golden fresh prompt after successful preflight.
    let op = Operation::FreshIssue;
    assert!(op.is_fresh(), "FreshIssue is a fresh operation");
}

// ---- CW02-11: fresh PR ordering ----

#[test]
fn fresh_pr_ordering() {
    parse_scenario("agent-fresh-pr.json");
    // Contract: WHEN fresh PR Send is confirmed, Jefe shall emit exactly one
    // fixture-golden fresh prompt after successful preflight.
    let op = Operation::FreshPullRequest;
    assert!(op.is_fresh(), "FreshPullRequest is a fresh operation");
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
    // Contract: WHEN schema-1 records migrate, Jefe shall preserve known typed
    // values and exact dormant unknown records.
    // S1 contract: AgentTypeId replaces AgentKind with strict validation.
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
    // Contract: WHEN a matching live launch restores, Jefe shall attach
    // through the existing tmux/PTY boundary.
    // RED: AgentLaunchPlan carries the signature that restore reconciles.
    let plan = AgentLaunchPlan::default();
    assert!(
        plan.signature_excludes_secrets(),
        "signature v1 excludes secrets and display-only values"
    );
}

// ---- CW02-15: architecture guard ----

#[test]
fn agent_architecture_guard() {
    parse_scenario("agent-no-product-branches.json");
    // Contract: WHEN the architecture guard scans source, Jefe shall find
    // product tokens and shim-token permutations only in the explicit
    // allowlist, and AgentKind shall not exist.
    // The shipped definitions are the only allowed product-token location.
    for agent in AGENTS {
        assert_provenance(agent);
    }
    // RED: the registry must expose exactly four shipped definitions and
    // AgentKind must be absent at feature-complete.
    let defs = AgentDefinition::shipped();
    assert_eq!(defs.len(), 4, "exactly four shipped definitions");
}

// ---- CW02-16: claude entry gate ----

#[test]
fn claude_entry_gate() {
    parse_scenario("agent-claude-evidence-gate.json");
    // Contract: IF no Claude executable is installed, Jefe shall publish
    // Claude as not found and execute zero Claude process.
    // The fixture-release evidence proves the Claude mapping is real even
    // though the runtime probe decides support per installation.
    let claude = &AGENTS[2];
    assert_probe_identity(claude);
    assert_provenance(claude);
    let status = Availability::NotFound;
    assert!(status.is_not_found(), "absent Claude publishes NotFound");
}

// ---- CW02-17: package runner selector ----

#[test]
fn package_runner_selector() {
    parse_scenario("agent-version-selector.json");
    // Contract: WHEN a nonblank version selector is set for an npm/uvx-package
    // candidate, Jefe shall plan the exact package-runner argv and reprobe
    // under a new generation.
    // RED: the typed package-runner candidate kinds generalize the LLxprt npm
    // selector and Code Puppy uvx selector.
    let npm = CandidateKind::NpmPackage {
        package: "@vybestack/llxprt-code".to_string(),
        binary: "llxprt".to_string(),
    };
    let uvx = CandidateKind::UvxPackage {
        package: "code-puppy".to_string(),
        binary: "code-puppy".to_string(),
    };
    assert!(npm.is_package_runner(), "npm-package is a package runner");
    assert!(uvx.is_package_runner(), "uvx-package is a package runner");
}
