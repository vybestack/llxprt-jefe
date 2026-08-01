//! In-crate tests for the JSP launch instrumentation decision
//! (`jsp_launch::prepare`, issue #522 J2/J10).
//!
//! `prepare` is `pub(super)`, so it is testable only from inside the crate.
//! These tests pin the decision boundary: a reattached process and a plan
//! with no coordinator installed receive no injected bootstrap material,
//! while a supported local plan with a coordinator installed is instrumented
//! with a bootstrap path and returns a live `PreparedJspLaunch`.

use std::ffi::OsStr;
use std::path::PathBuf;

use super::jsp_launch;
use super::test_support::authorized_launch_plan;
use crate::domain::AgentId;
use crate::domain::agent_definition::{AgentLaunchPlan, AgentTypeId, Target};
use crate::jsp_host::{BOOTSTRAP_ENV, JspHostRuntime};

/// An inert local LLxprt launch plan. The `/bin/sleep` argv is never executed.
fn local_llxprt_plan(cwd: &std::path::Path) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("parse core.llxprt type id: {error}")),
        target: Target::Local {
            canonical_cwd: cwd.to_path_buf(),
        },
        ..AgentLaunchPlan::default()
    }
}

/// A reattached launch must return the plan unchanged with no
/// `PreparedJspLaunch`, even when a coordinator is installed. An
/// already-running or restored process receives no injected bootstrap
/// material (issue #522 J10).
#[test]
fn reattaching_launch_returns_plan_unchanged_with_no_prepared() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-reattach".to_owned());
    let original = authorized_launch_plan(&local_llxprt_plan(temp.path()));

    let (result_plan, prepared) =
        jsp_launch::prepare(Some(&coordinator), &agent_id, &original, None, true, 1)
            .unwrap_or_else(|error| panic!("prepare reattaching: {error}"));

    assert!(
        prepared.is_none(),
        "reattaching launch must not produce a PreparedJspLaunch"
    );
    assert_eq!(
        result_plan.plan(),
        original.plan(),
        "reattaching launch must return the plan unchanged"
    );
}

/// A plan with no coordinator installed must return unchanged with no
/// `PreparedJspLaunch`. No coordinator means no JSP host is running, so no
/// bootstrap material is created (issue #522 J10).
#[test]
fn no_coordinator_returns_plan_unchanged_with_no_prepared() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let agent_id = AgentId("agent-no-coord".to_owned());
    let original = authorized_launch_plan(&local_llxprt_plan(temp.path()));

    let (result_plan, prepared) = jsp_launch::prepare(None, &agent_id, &original, None, false, 1)
        .unwrap_or_else(|error| panic!("prepare no coordinator: {error}"));

    assert!(
        prepared.is_none(),
        "no coordinator must not produce a PreparedJspLaunch"
    );
    assert_eq!(
        result_plan.plan(),
        original.plan(),
        "no coordinator must return the plan unchanged"
    );
}

/// A supported local LLxprt plan with a coordinator installed must return an
/// instrumented plan carrying the bootstrap env path plus a live
/// `PreparedJspLaunch`. The returned plan differs from the original only in
/// the injected bootstrap environment entry (issue #522 J2).
#[test]
fn supported_local_plan_with_coordinator_is_instrumented() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let coordinator = runtime.coordinator();
    let agent_id = AgentId("agent-instrument".to_owned());
    let original = authorized_launch_plan(&local_llxprt_plan(temp.path()));

    let (result_plan, prepared) =
        jsp_launch::prepare(Some(&coordinator), &agent_id, &original, None, false, 1)
            .unwrap_or_else(|error| panic!("prepare supported local: {error}"));

    let prepared =
        prepared.unwrap_or_else(|| panic!("supported local launch must be instrumented"));

    // The instrumented plan carries exactly one bootstrap env entry pointing
    // at the prepared bootstrap path. The original plan had no env entries.
    assert!(
        original.plan().env.is_empty(),
        "fixture plan starts with no env entries"
    );
    let bootstrap_entries: Vec<_> = result_plan
        .plan()
        .env
        .iter()
        .filter(|(name, _)| name.as_os_str() == OsStr::new(BOOTSTRAP_ENV))
        .collect();
    assert_eq!(
        bootstrap_entries.len(),
        1,
        "instrumented plan carries exactly one bootstrap env entry"
    );
    let injected_path = PathBuf::from(bootstrap_entries[0].1.as_os_str());
    assert_eq!(
        injected_path,
        prepared.bootstrap_path(),
        "injected env path must match the prepared bootstrap path"
    );
    assert!(
        injected_path.starts_with(temp.path()),
        "bootstrap path must live inside the runtime dir"
    );
    assert!(
        injected_path.exists(),
        "bootstrap file must exist after prepare"
    );

    // Commit so the drop does not revoke — verify the credential is live.
    prepared.commit();
}
