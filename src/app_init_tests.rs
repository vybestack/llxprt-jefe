//! Behavioural tests for startup restore and reconciliation.
//!
//! Extracted from `app_init.rs` to keep that file within the source-size
//! policy; the module is included via `#[path]` exactly as the runtime
//! manager's tests are.

use super::warnings::surface_unclean_prior_runs;
use super::*;
use jefe::domain::{Repository, RepositoryId, TypedValue, UncleanRun};
use jefe::runtime::RuntimeSession;
use std::time::Duration;

fn code_puppy_agent_and_repository() -> (Agent, Repository) {
    let repository_id = RepositoryId("repo-model".to_owned());
    let repository = Repository::new(
        repository_id.clone(),
        jefe::domain::shipped_agent_type(1),
        jefe::domain::TypedMap::new(),
        "Model Repo".to_owned(),
        "model-repo".to_owned(),
        std::path::PathBuf::from("/tmp/model-repo"),
    );
    let agent = Agent::new(
        AgentId("agent-model".to_owned()),
        repository_id,
        jefe::domain::shipped_agent_type(1),
        jefe::domain::TypedMap::new(),
        "Model Agent".to_owned(),
        std::path::PathBuf::from("/tmp/model-agent"),
    );
    (agent, repository)
}

fn set_string(values: &mut jefe::domain::TypedMap, field: &str, value: &str) {
    jefe::domain::canonical_values::insert_json(
        values,
        field,
        serde_json::Value::String(value.to_owned()),
    )
    .unwrap_or_else(|error| panic!("valid {field} fixture: {error}"));
}

#[test]
fn launch_request_uses_agent_type_values_and_repository_target() {
    let (mut agent, mut repository) = code_puppy_agent_and_repository();
    set_string(
        &mut repository.default_values,
        "model",
        "repo/default-model",
    );
    set_string(&mut agent.values, "model", "agent/model");
    repository.remote.host = "build.example.com".to_owned();

    let request = launch_signature_for_agent(&agent, &repository);

    assert_eq!(request.type_id, agent.type_id);
    assert_eq!(request.values, agent.values);
    assert_eq!(request.work_dir, agent.work_dir);
    assert_eq!(request.remote, repository.remote);
    assert_eq!(
        request.operation,
        jefe::domain::agent_definition::Operation::Resume
    );
    assert_eq!(
        jefe::domain::canonical_values::typed_field(&request.values, "model"),
        Some(&TypedValue::String("agent/model".to_owned()))
    );
}

#[test]
fn launch_request_does_not_dynamically_inherit_repository_values() {
    let (agent, mut repository) = code_puppy_agent_and_repository();
    set_string(
        &mut repository.default_values,
        "model",
        "repo/default-model",
    );

    let request = launch_signature_for_agent(&agent, &repository);

    assert!(jefe::domain::canonical_values::typed_field(&request.values, "model").is_none());
}

/// Ownership evidence is derived from the session name and process identity
/// only. Configuration content is a statement about the *next* launch and must
/// not participate (issue #583).
#[test]
fn binding_evidence_ignores_configuration_and_uses_only_ownership_anchors() {
    let (mut agent, repository) = code_puppy_agent_and_repository();
    let request = launch_signature_for_agent(&agent, &repository);
    let launched_with = jefe::runtime::launch_compose::launch_signature_from_request(&request)
        .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
    let mut binding = jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&agent.id),
        launch_signature: launched_with,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: Some(WorkerProcessIdentity::new(std::process::id(), 4_242)),
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    };

    assert_eq!(
        binding_evidence(Some(&binding), &agent.id),
        BindingEvidence::Coherent,
        "a coherent binding for the stable session is owned"
    );

    // Every kind of configuration drift leaves ownership untouched: a changed
    // value, a changed target, and a definition hash the running process
    // predates.
    set_string(&mut agent.values, "model", "changed-model");
    agent.work_dir = std::path::PathBuf::from("/tmp/changed-target");
    agent.persisted_launch_signature = Some(jefe::domain::LaunchSignatureV1::default());
    assert_eq!(
        binding_evidence(Some(&binding), &agent.id),
        BindingEvidence::Coherent,
        "configuration drift must never revoke ownership of a running process"
    );

    // A binding naming a different session is genuinely not ours.
    binding.session_name = "jefe-agent-other".to_owned();
    assert_eq!(
        binding_evidence(Some(&binding), &agent.id),
        BindingEvidence::Inconsistent
    );
}

/// A binding without a creation token cannot reject PID reuse, so it stays
/// Legacy rather than being trusted as coherent.
#[test]
fn binding_without_a_creation_token_is_legacy() {
    let (agent, repository) = code_puppy_agent_and_repository();
    let request = launch_signature_for_agent(&agent, &repository);
    let launched_with = jefe::runtime::launch_compose::launch_signature_from_request(&request)
        .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
    let binding = jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&agent.id),
        launch_signature: launched_with,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: None,
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    };

    assert_eq!(
        binding_evidence(Some(&binding), &agent.id),
        BindingEvidence::Legacy
    );
    assert_eq!(binding_evidence(None, &agent.id), BindingEvidence::Legacy);
}

/// Editing an agent field must never abandon that agent's live session.
///
/// A launch-signature field (here `model`; the version selector behaves
/// identically) is edited while the agent keeps running. The tmux session and
/// the worker process are both still alive, so jefe unambiguously still owns
/// this process. Configuration content changed; process ownership did not.
///
/// `Stopped`, `Stale`, `Inconsistent` and `Orphaned` all clear the runtime
/// binding without killing the session, which strands the live agent
/// permanently: nothing re-adopts a session whose record was cleared. Startup
/// must therefore reach a binding-preserving classification here.
#[test]
fn editing_a_launch_field_does_not_abandon_a_live_agent() {
    let (mut agent, repository) = code_puppy_agent_and_repository();
    let launch_request = launch_signature_for_agent(&agent, &repository);
    let launched_with =
        jefe::runtime::launch_compose::launch_signature_from_request(&launch_request)
            .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));

    // The agent is running: the binding and the durable record both carry the
    // signature stamped when the process was launched.
    agent.persisted_launch_signature = Some(launched_with.clone());
    agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&agent.id),
        launch_signature: launched_with,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: Some(WorkerProcessIdentity::new(std::process::id(), 4_242)),
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    });

    // The user edits a field. The running process is untouched.
    set_string(&mut agent.values, "model", "changed-model");

    let binding = binding_evidence(agent.runtime_binding.as_ref(), &agent.id);

    // Session and worker are both observably alive.
    let classification = classify_startup(
        SessionEvidence::Alive,
        binding,
        false,
        ProcessLiveness::Alive,
        jefe::runtime::OrphanClassification::NoOrphan,
    );

    assert!(
        !matches!(
            classification,
            StartupClassification::Stopped
                | StartupClassification::Stale
                | StartupClassification::Inconsistent
                | StartupClassification::Orphaned
        ),
        "editing a field must not abandon a live agent, but startup classified \
         an alive session with an alive worker as {classification:?}, which \
         clears the runtime binding and strands the still-running session"
    );
}

#[test]
fn legacy_pid_only_binding_uses_conservative_native_probe() {
    // A legacy binding carries a worker PID with no creation token, so the
    // probe falls back to a bare liveness question (issue #543).
    let pid = std::process::id();
    assert_eq!(
        process_liveness_for_binding(Some(WorkerProcessIdentity::from_pid(pid))),
        ProcessLiveness::Alive
    );
    assert_eq!(
        process_liveness_for_binding(Some(WorkerProcessIdentity::from_pid(2_000_000_000))),
        ProcessLiveness::Dead
    );
    assert_eq!(
        process_liveness_for_binding(None),
        ProcessLiveness::MalformedIdentity
    );
}

/// A token-less binding must report the probe's real answer. Collapsing it to
/// a `bool` turned "could not look" into a positive claim of liveness, so the
/// same uncertainty was held when a creation token happened to be present and
/// asserted as alive when it was not (issue #541).
#[test]
fn a_token_less_binding_reports_uncertainty_rather_than_asserting_life() {
    use jefe::runtime::{ProcessObservation, classify_process_observation};

    let legacy = WorkerProcessIdentity::from_pid(4321).identity();

    for (observation, expected) in [
        (
            ProcessObservation::Inaccessible,
            ProcessLiveness::Inaccessible,
        ),
        (
            ProcessObservation::ProbeFailed,
            ProcessLiveness::ProbeFailure,
        ),
        (ProcessObservation::Exited, ProcessLiveness::Dead),
    ] {
        assert_eq!(
            classify_process_observation(Some(legacy), observation),
            expected,
            "a token-less binding must not report {observation:?} as a verdict about life"
        );
    }
}

#[test]
fn startup_classification_covers_required_lifecycle_states() {
    use jefe::runtime::OrphanClassification as Oc;
    // Local helper: fix remote=false and orphan=NoOrphan so each row is a
    // compact (session, binding, process) -> expected assertion.
    let cls = |session, process, expected| {
        assert_eq!(
            classify_startup(
                session,
                BindingEvidence::Coherent,
                false,
                process,
                Oc::NoOrphan
            ),
            expected
        );
    };
    cls(
        SessionEvidence::Alive,
        ProcessLiveness::Dead,
        StartupClassification::Running,
    );
    cls(
        SessionEvidence::Missing,
        ProcessLiveness::Dead,
        StartupClassification::Stopped,
    );
    cls(
        SessionEvidence::Missing,
        ProcessLiveness::ReusedPid,
        StartupClassification::Stale,
    );
    cls(
        SessionEvidence::Alive,
        ProcessLiveness::ReusedPid,
        StartupClassification::Stale,
    );
    cls(
        SessionEvidence::Missing,
        ProcessLiveness::Alive,
        StartupClassification::Recoverable,
    );
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Inconsistent,
            false,
            ProcessLiveness::Alive,
            Oc::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
}

/// An unanswered session probe is held, not classified.
///
/// This previously asserted `Recoverable`, which kept the agent alive but was
/// a *conclusion* drawn from evidence that never arrived, and was
/// indistinguishable from the same conclusion reached from real evidence.
/// `Held` is strictly more conservative and, unlike `Recoverable`, records
/// that the question is still open so it can be asked again (issue #541).
#[test]
fn an_unanswered_session_probe_is_held_not_classified() {
    for liveness in [ProcessLiveness::Dead, ProcessLiveness::ProbeFailure] {
        let classification = classify_startup(
            SessionEvidence::Unavailable,
            BindingEvidence::Coherent,
            false,
            liveness,
            jefe::runtime::OrphanClassification::NoOrphan,
        );
        assert_eq!(
            classification,
            StartupClassification::Held,
            "an unavailable session probe must not reach a verdict"
        );
        assert_ne!(
            classification,
            StartupClassification::Stopped,
            "the original guarantee still holds: no phantom death"
        );
    }
}

#[test]
fn missing_remote_session_is_stopped_without_local_pid_fallback() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Coherent,
            true,
            ProcessLiveness::Alive,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Stopped
    );
}

#[test]
fn malformed_or_inaccessible_process_identity_is_classified_conservatively() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Coherent,
            false,
            ProcessLiveness::MalformedIdentity,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
    // A process we are not permitted to inspect has not told us it is dead.
    // That is an unanswered question, not a recoverable conclusion.
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Coherent,
            false,
            ProcessLiveness::Inaccessible,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Held
    );
}

/// The mirror hazard for #541: holding must not swallow real evidence.
///
/// A change that held on everything would satisfy the invariant and destroy
/// the feature, so the answered paths are pinned alongside the held ones.
#[test]
fn answered_evidence_still_reaches_every_verdict() {
    let answered = [
        (
            SessionEvidence::Alive,
            ProcessLiveness::Alive,
            StartupClassification::Running,
        ),
        (
            SessionEvidence::Missing,
            ProcessLiveness::Dead,
            StartupClassification::Stopped,
        ),
        (
            SessionEvidence::Missing,
            ProcessLiveness::ReusedPid,
            StartupClassification::Stale,
        ),
        (
            SessionEvidence::Missing,
            ProcessLiveness::MalformedIdentity,
            StartupClassification::Inconsistent,
        ),
    ];
    for (session, process, expected) in answered {
        assert_eq!(
            classify_startup(
                session,
                BindingEvidence::Coherent,
                false,
                process,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            expected,
            "answered evidence must still produce a verdict"
        );
    }
}

/// PID reuse still overrides a live session. Removing the configuration
/// comparison must not weaken the ownership checks that remain (issue #583).
#[test]
fn reused_pid_still_overrides_a_live_session() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Alive,
            BindingEvidence::Coherent,
            false,
            ProcessLiveness::ReusedPid,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Stale
    );
}

#[test]
fn live_session_with_inconsistent_binding_is_rejected() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Alive,
            BindingEvidence::Inconsistent,
            false,
            ProcessLiveness::Alive,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
}

#[test]
fn missing_session_with_inconsistent_binding_still_inconsistent() {
    // Negative case: without a live session there is nothing to rescue,
    // so the Inconsistent classification is preserved (existing behavior).
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Inconsistent,
            false,
            ProcessLiveness::Alive,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Inconsistent,
            true,
            ProcessLiveness::Alive,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
}

#[test]
fn published_agent_enablement_is_separate_from_availability() {
    let catalog = jefe::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog must publish: {error}"));
    let migration = jefe::persistence::migration::migrate_settings(
        b"settings_schema = 2\n[agents.\"core.codex\"]\nenabled = false\n",
        &catalog,
    )
    .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"));
    let type_id = jefe::domain::agent_definition::AgentTypeId::parse("core.codex")
        .unwrap_or_else(|error| panic!("type id must parse: {error}"));

    assert!(!jefe::agent_registry::agent_type_enabled(
        migration.published(),
        &type_id
    ));

    let absent = jefe::domain::agent_definition::AgentTypeId::parse("core.llxprt")
        .unwrap_or_else(|error| panic!("type id must parse: {error}"));
    assert!(jefe::agent_registry::agent_type_enabled(
        migration.published(),
        &absent
    ));
}

// -- Startup multiplexer qualification (issue #540 req 3) -----------------

/// The issue's complaint is that the gate ran at first `new-session`, so a user
/// could launch jefe, navigate the UI, and only discover the multiplexer was
/// unusable when starting an agent. A refusal must therefore surface at
/// startup, carrying the detail needed to act on it.
#[cfg(windows)]
#[test]
fn a_refused_multiplexer_is_reported_at_startup() {
    let qualification = jefe::runtime::MultiplexerQualification::Refused {
        message: "psmux at C:/tools/psmux.exe (3.3.6) lacks set-option -s exit-empty".to_owned(),
    };

    let warning = startup_multiplexer_warning(&qualification)
        .unwrap_or_else(|| panic!("a refusal must reach the operator at startup"));

    assert!(warning.contains("C:/tools/psmux.exe"), "{warning}");
    assert!(warning.contains("exit-empty"), "{warning}");
}

/// A qualified binary with nothing wrong produces no noise, or the warning
/// surface stops being read.
#[cfg(windows)]
#[test]
fn a_qualified_multiplexer_is_silent() {
    let qualification = jefe::runtime::MultiplexerQualification::Qualified {
        report: jefe::runtime::ConformanceReport::default(),
    };

    assert_eq!(startup_multiplexer_warning(&qualification), None);
}

/// #537 in miniature: one transient failure at the startup boundary must not
/// be enough to withhold a verdict about a live agent.
#[test]
fn a_startup_session_probe_that_recovers_is_believed() {
    let mut calls = 0_u32;

    let evidence = retry_session_evidence(RetryPolicy::new(3, Duration::from_millis(0)), || {
        calls += 1;
        if calls < 3 {
            SessionLiveness::Unavailable
        } else {
            SessionLiveness::Alive
        }
    });

    assert_eq!(
        evidence,
        SessionEvidence::Alive,
        "a probe that answered on retry must be believed"
    );
    assert_eq!(calls, 3);
}

/// When the probe never answers the honest result is still `Unavailable`, and
/// the retry must be bounded rather than blocking startup indefinitely.
#[test]
fn a_startup_session_probe_that_never_answers_stays_unavailable() {
    let mut calls = 0_u32;

    let evidence = retry_session_evidence(RetryPolicy::new(3, Duration::from_millis(0)), || {
        calls += 1;
        SessionLiveness::Unavailable
    });

    assert_eq!(evidence, SessionEvidence::Unavailable);
    assert_eq!(calls, 3, "retries are bounded");
}

/// A session that is genuinely gone has *answered*. Re-asking would delay
/// every dead agent at startup and buy nothing.
#[test]
fn a_missing_session_is_not_retried() {
    let mut calls = 0_u32;

    let evidence = retry_session_evidence(RetryPolicy::new(5, Duration::from_millis(0)), || {
        calls += 1;
        SessionLiveness::Missing
    });

    assert_eq!(evidence, SessionEvidence::Missing);
    assert_eq!(calls, 1, "an answered probe is asked exactly once");
}

/// V3 / #527: twenty live panes were marked stopped because a hash changed.
/// A signature that cannot be composed describes the configuration, not the
/// process, so a live agent must be held rather than buried.
#[test]
fn a_live_agent_whose_signature_will_not_compose_is_held_not_dead() {
    let outcome = signature_for_running_agent(None, None);

    let reason = outcome
        .err()
        .unwrap_or_else(|| panic!("an uncomposable signature must not yield a signature"));
    assert_eq!(reason.boundary(), ProbeBoundary::LaunchSignature);
}

/// The signature the process was actually launched with wins over whatever the
/// current configuration would produce (issue #583).
#[test]
fn a_persisted_signature_beats_the_current_configuration() {
    let persisted = jefe::domain::LaunchSignatureV1::default();
    let composed = jefe::domain::LaunchSignatureV1::default();

    let chosen = signature_for_running_agent(Some(persisted.clone()), Some(composed))
        .unwrap_or_else(|reason| panic!("a persisted signature must be adopted: {reason}"));

    assert_eq!(chosen, persisted);
}

/// The mirror hazard: when the configuration *can* compose a signature and
/// nothing was persisted, adoption must still proceed rather than hold.
#[test]
fn a_composable_signature_is_still_adopted() {
    let composed = jefe::domain::LaunchSignatureV1::default();

    let chosen = signature_for_running_agent(None, Some(composed.clone()))
        .unwrap_or_else(|reason| panic!("a composable signature must be adopted: {reason}"));

    assert_eq!(chosen, composed);
}

/// A held durable read pauses saving. If that is not on screen the operator
/// sees a working jefe that is quietly not persisting anything, which is worse
/// than the crash it replaced: the failure is invisible until the work is
/// already lost (issue #541 V7).
#[test]
fn a_held_durable_read_is_visible_to_the_operator() {
    let mut state = crate::test_app_state();

    surface_durable_read_hold(&mut state, Some("state.json is not valid JSON".to_owned()));

    let shown = state
        .warning_message
        .as_ref()
        .unwrap_or_else(|| panic!("a held durable read must reach the operator, not just the log"));
    assert!(
        shown.contains("state.json is not valid JSON"),
        "the operator needs the reason, not just that something is wrong: {shown}"
    );
    assert_eq!(
        state.durable_read_held,
        Some("state.json is not valid JSON".to_owned()),
        "the hold itself must stay set so writes remain paused"
    );
}

/// The mirror hazard: a readable document must leave no warning and no hold.
#[test]
fn a_successful_durable_read_shows_nothing() {
    let mut state = crate::test_app_state();

    surface_durable_read_hold(&mut state, None);

    assert_eq!(state.warning_message, None);
    assert_eq!(state.durable_read_held, None);
}

/// A held agent is left exactly as persisted -- Running, with no binding. The
/// liveness cycle builds its targets from the runtime's session map, so an
/// agent that was never registered produces no target and is never probed
/// again. Without a visible warning the operator sees a Running agent that
/// cannot be attached to and is given no reason (issue #541 V4/V7).
#[test]
fn held_agents_are_reported_to_the_operator() {
    let mut state = crate::test_app_state();

    surface_startup_holds(
        &mut state,
        &[
            (
                AgentId("agent-1".to_owned()),
                "has-session did not answer: server unreachable".to_owned(),
            ),
            (
                AgentId("agent-2".to_owned()),
                "has-session did not answer: server unreachable".to_owned(),
            ),
        ],
    );

    let shown = state
        .warning_message
        .as_ref()
        .unwrap_or_else(|| panic!("held agents must be reported, not only logged"));
    assert!(
        shown.contains('2'),
        "the operator needs to know how many agents are affected: {shown}"
    );
    assert!(
        shown.contains("server unreachable"),
        "the operator needs the reason, not just a count: {shown}"
    );
}

/// The mirror hazard: no holds must leave the status line alone.
#[test]
fn no_holds_reports_nothing() {
    let mut state = crate::test_app_state();
    surface_startup_holds(&mut state, &[]);
    assert_eq!(state.warning_message, None);
}

/// A held agent keeps its Running status but has no binding, and the liveness
/// cycle builds its targets from the runtime's session map -- so nothing
/// probes it again. Selecting exactly these agents is what lets a later pass
/// give them an eventual verdict instead of leaving them phantoms (#541 V4).
#[test]
fn agents_held_at_startup_are_selected_for_another_attempt() {
    let (mut agent, _repo) = code_puppy_agent_and_repository();
    agent.status = AgentStatus::Running;
    agent.runtime_binding = None;

    let expected = agent.id.clone();
    let mut state = crate::test_app_state();
    state.agents = vec![agent];

    assert_eq!(
        agents_awaiting_readoption(&state),
        vec![expected],
        "a Running agent with no binding is exactly the phantom that needs re-probing"
    );
}

/// The mirror hazard, twice over: re-probing must not disturb an agent that is
/// already bound, and must not resurrect one that was answered as dead.
#[test]
fn bound_and_finished_agents_are_left_alone() {
    let (mut bound, repo) = code_puppy_agent_and_repository();
    bound.status = AgentStatus::Running;
    let request = AgentLaunchRequest::for_agent(&bound, &repo);
    let launched_with = jefe::runtime::launch_compose::launch_signature_from_request(&request)
        .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
    bound.runtime_binding = Some(jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&bound.id),
        launch_signature: launched_with,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: None,
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    });

    let (mut dead, _repo2) = code_puppy_agent_and_repository();
    dead.id = AgentId("agent-2".to_owned());
    dead.status = AgentStatus::Dead;
    dead.runtime_binding = None;

    let mut state = crate::test_app_state();
    state.agents = vec![bound, dead];

    assert!(
        agents_awaiting_readoption(&state).is_empty(),
        "only unbound Running agents are unresolved; a bound one is answered and a dead one is finished"
    );
}

/// A missing repository is jefe failing to find configuration, not the agent's
/// process ending. Burying it was the last instance of the #527 collapse, and
/// it matters more now that the periodic re-adoption pass reaches this code on
/// every cycle rather than once at startup (issue #541 V3).
#[test]
fn an_agent_whose_repository_is_missing_is_held_not_buried() {
    let (mut agent, _repo) = code_puppy_agent_and_repository();
    agent.status = AgentStatus::Running;

    let mut runtime = TmuxRuntimeManager::new(24, 80);
    let outcome = restore_one_agent(&agent, &[], &mut runtime, None);

    assert!(
        matches!(outcome, RestoreOneOutcome::Held(_)),
        "a repository we cannot find says nothing about whether the process is alive"
    );
}

/// An orphan is a dead pane whose validated worker descendants are still
/// running, so it is the one terminal classification that has work to do
/// before the agent is written off. The plain Dead route clears the runtime
/// binding, and `reap_orphaned_agent` returns immediately without one, so
/// folding Orphaned into Dead destroys the anchors the reap depends on and the
/// leftover process tree survives every restart (issue #642).
#[test]
fn an_orphan_does_not_take_the_same_restore_route_as_a_plain_dead_agent() {
    assert!(
        !matches!(
            terminal_restore_outcome(StartupClassification::Orphaned),
            RestoreOneOutcome::Dead
        ),
        "Orphaned must stay distinguishable from Dead, or the reap is skipped"
    );
}

/// Issue #642 AC5: the reap must see the orphan while it is still bound.
///
/// Burying an agent clears its runtime binding, and that binding holds the only
/// anchors `reap_orphan_session` can match a surviving process against. So it is
/// not enough that a reap happens somewhere on the orphan route — it has to
/// happen while the anchors are still reachable, and the agent must still end up
/// buried afterwards.
#[test]
fn an_orphan_is_reaped_while_it_still_carries_its_anchors() {
    let (mut agent, repo) = code_puppy_agent_and_repository();
    agent.status = AgentStatus::Running;
    let request = AgentLaunchRequest::for_agent(&agent, &repo);
    let launched_with = jefe::runtime::launch_compose::launch_signature_from_request(&request)
        .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
    let anchors = vec![jefe::domain::WorkerProcessIdentity::new(4310, 111)];
    agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&agent.id),
        launch_signature: launched_with,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: None,
        lifecycle_generation: 0,
        worker_identities: anchors.clone(),
    });

    let mut sets = RestoreOutcomeSets::default();
    let mut anchors_visible_to_the_reap = None;
    record_restore_outcome(
        &agent,
        RestoreOneOutcome::Orphaned,
        &mut sets,
        &mut |orphan| {
            anchors_visible_to_the_reap = orphan
                .runtime_binding
                .as_ref()
                .map(|binding| binding.worker_identities.clone());
        },
    );

    assert_eq!(
        anchors_visible_to_the_reap,
        Some(anchors),
        "the reap must run before the bury, while the binding still names the tree to kill"
    );
    assert_eq!(
        sets.newly_dead,
        vec![agent.id.clone()],
        "reaping the tree does not excuse the agent from being buried"
    );
}

/// The three classifications that carry no surviving descendants are genuinely
/// finished and must keep taking the binding-clearing Dead route, so splitting
/// the orphan out does not quietly strand ordinary dead agents.
#[test]
fn terminal_classifications_without_orphans_still_mark_the_agent_dead() {
    for classification in [
        StartupClassification::Stopped,
        StartupClassification::Stale,
        StartupClassification::Inconsistent,
    ] {
        assert!(
            matches!(
                terminal_restore_outcome(classification),
                RestoreOneOutcome::Dead
            ),
            "{classification:?} has nothing left to reap and must still be marked Dead"
        );
    }
}

fn vanished_run(pid: u32, last_seen_unix: u64, breadcrumb: Option<&str>) -> UncleanRun {
    UncleanRun {
        pid,
        version: "0.0.32".to_owned(),
        last_seen_unix,
        breadcrumb: breadcrumb.map(str::to_owned),
    }
}

#[test]
fn a_prior_run_that_vanished_is_named_where_the_operator_will_see_it() {
    let mut state = crate::test_app_state();

    surface_unclean_prior_runs(
        &mut state,
        &[vanished_run(4242, 1_000, Some("attach agent-7"))],
        1_130,
    );

    let Some(warning) = state.warning_message.as_deref() else {
        panic!("a run that vanished must be reported in the UI, not only in the log");
    };
    assert!(
        warning.contains("(pid 4242)"),
        "must name the pid: {warning}"
    );
    assert!(
        warning.contains("(unix 1000)"),
        "must name the last-seen timestamp: {warning}"
    );
    assert!(
        warning.contains("attach agent-7"),
        "must name what it was doing: {warning}"
    );
}

#[test]
fn a_start_with_nothing_left_behind_says_nothing() {
    let mut state = crate::test_app_state();

    surface_unclean_prior_runs(&mut state, &[], 1_130);

    assert_eq!(
        state.warning_message, None,
        "a clean start must not invent a warning"
    );
}

#[test]
fn every_run_that_vanished_is_reported_not_only_the_first() {
    let mut state = crate::test_app_state();

    surface_unclean_prior_runs(
        &mut state,
        &[
            vanished_run(4242, 1_000, None),
            vanished_run(9001, 1_020, None),
        ],
        1_130,
    );

    let Some(warning) = state.warning_message.as_deref() else {
        panic!("both runs that vanished must be reported");
    };
    assert!(
        warning.contains("(pid 4242)"),
        "first pid missing: {warning}"
    );
    assert!(
        warning.contains("(pid 9001)"),
        "second pid missing: {warning}"
    );
}

#[test]
fn a_vanished_run_report_is_added_to_an_existing_warning_rather_than_replacing_it() {
    let mut state = crate::test_app_state();
    append_warning(&mut state, "Settings were reset.".to_owned());

    surface_unclean_prior_runs(&mut state, &[vanished_run(4242, 1_000, None)], 1_130);

    let Some(warning) = state.warning_message.as_deref() else {
        panic!("the existing warning must survive");
    };
    assert!(
        warning.contains("Settings were reset."),
        "existing warning was lost: {warning}"
    );
    assert!(
        warning.contains("(pid 4242)"),
        "new report missing: {warning}"
    );
}
