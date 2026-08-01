//! Issue #522 acceptance rows J2, J9, J10: JSP launch lifecycle through the
//! public coordinator API.
//!
//! These tests are fully deterministic: no tmux, no process spawning, no
//! network, no sleeps. They exercise the public [`JspLaunchCoordinator`] API
//! through [`JspHostRuntime`] and verify bootstrap material by inspecting file
//! PATHS only (counting `jsp-*.json` entries in the temp runtime directory).
//! No bootstrap file's contents are ever read, and no credential or bearer
//! token ever reaches an assertion message or log.

use std::path::Path;

use jefe::domain::AgentId;
use jefe::domain::agent_definition::{AgentLaunchPlan, AgentTypeId, RemoteTarget, Target};
use jefe::jsp_host::JspHostRuntime;
use jefe::runtime::agent_execution_guard::{
    AuthorizationResult, ExecutionEvidence, authorize_execution,
};
use jefe::runtime::agent_preflight::{
    AuthorizedLaunchPlan, PreparationOutcome, ProcessSandboxInspector, prepare_execution,
};

/// Count `jsp-*.json` bootstrap files (excluding temp `.jsp-*.tmp` staging
/// files) directly under `dir`. Returns the number of committed bootstrap
/// material files present.
fn count_bootstrap_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read runtime dir: {error}"))
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("jsp-") && name.ends_with(".json")
        })
        .count()
}

/// An inert local LLxprt launch plan. Its `/bin/sleep` argv is never executed
/// by these tests — only the type/target drive the coordinator's
/// `launch_supports_jsp` gate.
fn local_llxprt_plan(cwd: &Path) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("parse core.llxprt type id: {error}")),
        target: Target::Local {
            canonical_cwd: cwd.to_path_buf(),
        },
        ..AgentLaunchPlan::default()
    }
}

/// An inert plan whose agent type is not the supported local LLxprt type.
fn local_non_llxprt_plan(cwd: &Path) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.codex")
            .unwrap_or_else(|error| panic!("parse core.codex type id: {error}")),
        target: Target::Local {
            canonical_cwd: cwd.to_path_buf(),
        },
        ..AgentLaunchPlan::default()
    }
}

/// An inert LLxprt-typed plan with a Remote target.
fn remote_llxprt_plan() -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("parse core.llxprt type id: {error}")),
        target: Target::Remote(RemoteTarget::default()),
        ..AgentLaunchPlan::default()
    }
}

/// Seal a fixture plan into an [`AuthorizedLaunchPlan`] through the real
/// authorize + preflight proof chain (issue #382 S8→S10→seal), exactly as
/// production does. The evidence is derived from the plan's own
/// generation-bearing fields so the fixture's defaults authorize trivially.
fn authorized(plan: &AgentLaunchPlan) -> AuthorizedLaunchPlan {
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        plan.probe_generation,
        plan.target_generation,
        plan.activation_generation,
    );
    let authorized = match authorize_execution(plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(error) => panic!("fixture plan must authorize: {error}"),
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => {
            panic!("fixture plan must clear preflight: {reason}")
        }
    };
    AuthorizedLaunchPlan::from_cleared(cleared, plan.clone(), evidence)
        .unwrap_or_else(|error| panic!("fixture plan must seal: {error}"))
}

// ===========================================================================
// J2: Fresh local LLxprt launch — prepare creates one bootstrap file; a
// dropped (uncommitted) PreparedJspLaunch revokes the credential and removes
// the bootstrap file, leaving zero stranded material; a subsequent prepare
// for a new generation then succeeds. This is exactly what happens when the
// post-instrumentation preflight or the spawn fails.
// ===========================================================================

#[test]
fn j2_prepare_creates_one_bootstrap_and_drop_without_commit_revokes_and_removes() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-j2".to_owned());
    let plan = authorized(&local_llxprt_plan(temp.path()));

    // Preparing a launch creates exactly one bootstrap file. Dropping the
    // PreparedJspLaunch WITHOUT commit revokes the credential and removes
    // the bootstrap file — the failed-spawn / failed-preflight path. This
    // is exactly what happens when the post-instrumentation preflight or the
    // spawn fails. No credential or file content reaches this assertion.
    {
        let prepared = coordinator
            .prepare_launch(&agent_id, 1, plan.plan())
            .unwrap_or_else(|error| panic!("prepare launch gen 1: {error}"))
            .unwrap_or_else(|| panic!("local LLxprt launch must be instrumented"));
        let path = prepared.bootstrap_path();
        assert!(path.exists(), "bootstrap file must exist after prepare");
        assert!(
            path.starts_with(temp.path()),
            "bootstrap must live inside the runtime dir"
        );
        assert_eq!(
            count_bootstrap_files(&temp.path().join("jsp")),
            1,
            "exactly one bootstrap file after prepare"
        );
        // Drop without commit: simulates failed spawn or post-instrumentation
        // preflight failure. The credential is revoked and the bootstrap file
        // is removed automatically.
    }

    // After the dropped prepare, zero bootstrap material must remain.
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        0,
        "dropping an uncommitted prepared launch removes all bootstrap material"
    );

    // A subsequent prepare for a new generation succeeds, proving the agent
    // is not stranded after the revoked attempt.
    let prepared = coordinator
        .prepare_launch(&agent_id, 2, plan.plan())
        .unwrap_or_else(|error| panic!("prepare launch gen 2: {error}"))
        .unwrap_or_else(|| panic!("local LLxprt launch gen 2 must be instrumented"));
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        1,
        "exactly one bootstrap file after new-generation prepare"
    );
    // Commit so drop does not revoke — this is the successful spawn path.
    prepared.commit();
}

// ===========================================================================
// J9: Lifecycle — after commit, revoke removes the bootstrap material; a
// relaunch (prepare+commit for a higher generation) leaves exactly one
// bootstrap file and retires the previous generation's credential;
// revocation is agent-scoped so revoking agent A leaves agent B's material
// untouched.
// ===========================================================================

#[test]
fn j9_commit_then_revoke_removes_bootstrap_and_relaunch_retires_previous_credential() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-j9".to_owned());
    let plan = authorized(&local_llxprt_plan(temp.path()));

    // Commit generation 1 (successful spawn).
    coordinator
        .prepare_launch(&agent_id, 1, plan.plan())
        .unwrap_or_else(|error| panic!("prepare launch gen 1: {error}"))
        .unwrap_or_else(|| panic!("local LLxprt launch must be instrumented"))
        .commit();

    // Relaunch: prepare+commit for a higher generation leaves exactly one
    // bootstrap file and retires the previous generation's credential.
    let replacement_path = coordinator
        .prepare_launch(&agent_id, 2, plan.plan())
        .unwrap_or_else(|error| panic!("prepare launch gen 2: {error}"))
        .unwrap_or_else(|| panic!("local LLxprt launch gen 2 must be instrumented"));
    replacement_path.commit();
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        1,
        "relaunch leaves exactly one bootstrap file (previous retired)"
    );

    // After commit, revoke removes the bootstrap material.
    coordinator
        .revoke(&agent_id)
        .unwrap_or_else(|error| panic!("revoke agent after commit: {error}"));
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        0,
        "revoke after commit removes all bootstrap material"
    );
}

#[test]
fn j9_revocation_is_agent_scoped() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_a = AgentId("agent-j9-a".to_owned());
    let agent_b = AgentId("agent-j9-b".to_owned());
    let plan = authorized(&local_llxprt_plan(temp.path()));

    // Commit both agents' launches.
    coordinator
        .prepare_launch(&agent_a, 1, plan.plan())
        .unwrap_or_else(|error| panic!("prepare agent A gen 1: {error}"))
        .unwrap_or_else(|| panic!("agent A launch must be instrumented"))
        .commit();
    coordinator
        .prepare_launch(&agent_b, 1, plan.plan())
        .unwrap_or_else(|error| panic!("prepare agent B gen 1: {error}"))
        .unwrap_or_else(|| panic!("agent B launch must be instrumented"))
        .commit();
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        2,
        "two agents => two bootstrap files"
    );

    // Revoking agent A leaves agent B's material untouched (agent-scoped).
    coordinator
        .revoke(&agent_a)
        .unwrap_or_else(|error| panic!("revoke agent A: {error}"));
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        1,
        "revoking agent A leaves agent B's bootstrap untouched"
    );

    // Agent B can still be revoked independently.
    coordinator
        .revoke(&agent_b)
        .unwrap_or_else(|error| panic!("revoke agent B: {error}"));
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        0,
        "revoking agent B removes its material"
    );
}

// ===========================================================================
// J10: Unsupported paths — Remote launches and non-local-LLxprt-typed plans
// yield Ok(None) from prepare_launch and create no bootstrap material at all.
// ===========================================================================

#[test]
fn j10_remote_target_yields_none_and_creates_no_bootstrap() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-j10-remote".to_owned());
    let plan = authorized(&remote_llxprt_plan());

    let result = coordinator
        .prepare_launch(&agent_id, 1, plan.plan())
        .unwrap_or_else(|error| panic!("prepare remote launch: {error}"));
    assert!(
        result.is_none(),
        "remote target must yield Ok(None) — no bootstrap injection over SSH"
    );
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        0,
        "remote target creates no bootstrap material"
    );
}

#[test]
fn j10_non_llxprt_type_yields_none_and_creates_no_bootstrap() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-j10-codex".to_owned());
    let plan = authorized(&local_non_llxprt_plan(temp.path()));

    let result = coordinator
        .prepare_launch(&agent_id, 1, plan.plan())
        .unwrap_or_else(|error| panic!("prepare non-llxprt launch: {error}"));
    assert!(
        result.is_none(),
        "non-LLxprt agent type must yield Ok(None) — no bootstrap for unsupported agent type"
    );
    assert_eq!(
        count_bootstrap_files(&temp.path().join("jsp")),
        0,
        "non-LLxprt agent type creates no bootstrap material"
    );
}
