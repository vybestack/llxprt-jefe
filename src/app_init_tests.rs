//! Behavioural tests for startup restore and reconciliation.
//!
//! Extracted from `app_init.rs` to keep that file within the source-size
//! policy; the module is included via `#[path]` exactly as the runtime
//! manager's tests are.

use super::*;
use jefe::domain::{Repository, RepositoryId, TypedValue};
use jefe::runtime::RuntimeSession;

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

    assert!(!agent_type_enabled(migration.published(), &type_id));

    let absent = jefe::domain::agent_definition::AgentTypeId::parse("core.llxprt")
        .unwrap_or_else(|error| panic!("type id must parse: {error}"));
    assert!(agent_type_enabled(migration.published(), &absent));
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

    let warning = startup_multiplexer_warning(&qualification, None)
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

    assert_eq!(
        startup_multiplexer_warning(
            &qualification,
            Some(&jefe::runtime::ProvenanceVerdict::Qualified)
        ),
        None,
    );
}

/// Provenance is checked at startup alongside version and conformance, so an
/// unrecognised binary is reported even when it behaves correctly. Behaving
/// correctly is not evidence of being the binary jefe qualified.
#[cfg(windows)]
#[test]
fn an_unqualified_provenance_is_reported_even_when_conformance_passes() {
    let qualification = jefe::runtime::MultiplexerQualification::Qualified {
        report: jefe::runtime::ConformanceReport::default(),
    };
    let provenance = jefe::runtime::ProvenanceVerdict::Unqualified {
        diagnostic: "the multiplexer on PATH is not one jefe has qualified: C:/x/psmux.exe"
            .to_owned(),
    };

    let warning = startup_multiplexer_warning(&qualification, Some(&provenance))
        .unwrap_or_else(|| panic!("unknown provenance must be surfaced"));

    assert!(warning.contains("C:/x/psmux.exe"), "{warning}");
}

/// Both problems at once must both be reported; showing only the first would
/// send the operator round the loop twice.
#[cfg(windows)]
#[test]
fn conformance_and_provenance_problems_are_both_reported() {
    let qualification = jefe::runtime::MultiplexerQualification::Refused {
        message: "missing display-message -p".to_owned(),
    };
    let provenance = jefe::runtime::ProvenanceVerdict::Unqualified {
        diagnostic: "unrecognised digest deadbeef".to_owned(),
    };

    let warning = startup_multiplexer_warning(&qualification, Some(&provenance))
        .unwrap_or_else(|| panic!("both problems must surface"));

    assert!(warning.contains("display-message"), "{warning}");
    assert!(warning.contains("deadbeef"), "{warning}");
}
