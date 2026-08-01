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

#[test]
fn durable_signature_distinguishes_definition_drift_from_value_and_target_changes() {
    let (mut agent, repository) = code_puppy_agent_and_repository();
    let current = jefe::state::durable_projection::current_launch_signature(&agent, &repository)
        .unwrap_or_else(|error| panic!("fixture signature must project: {error}"));
    agent.persisted_launch_signature = Some(current.clone());
    assert_eq!(
        durable_signature_evidence(&agent, &repository),
        DurableSignatureEvidence::Match
    );

    let mut previous_definition = current;
    previous_definition.definition_hash =
        jefe::domain::LaunchSignatureV1::default().definition_hash;
    agent.persisted_launch_signature = Some(previous_definition);
    assert_eq!(
        durable_signature_evidence(&agent, &repository),
        DurableSignatureEvidence::DefinitionDrift
    );

    set_string(&mut agent.values, "model", "changed-model");
    assert_eq!(
        durable_signature_evidence(&agent, &repository),
        DurableSignatureEvidence::Inconsistent
    );
    agent.values.clear();
    agent.work_dir = std::path::PathBuf::from("/tmp/changed-target");
    assert_eq!(
        durable_signature_evidence(&agent, &repository),
        DurableSignatureEvidence::Inconsistent
    );
}

#[test]
fn binding_accepts_only_definition_drift_for_the_stable_session() {
    let (mut agent, repository) = code_puppy_agent_and_repository();
    let request = launch_signature_for_agent(&agent, &repository);
    let current = jefe::runtime::launch_compose::launch_signature_from_request(&request)
        .unwrap_or_else(|error| panic!("fixture signature must compose: {error}"));
    let mut previous_definition = current;
    previous_definition.definition_hash =
        jefe::domain::LaunchSignatureV1::default().definition_hash;
    agent.persisted_launch_signature = Some(previous_definition.clone());
    let mut binding = jefe::domain::RuntimeBinding {
        session_name: RuntimeSession::session_name_for(&agent.id),
        launch_signature: previous_definition,
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: None,
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    };
    let durable = durable_signature_evidence(&agent, &repository);

    assert_eq!(durable, DurableSignatureEvidence::DefinitionDrift);
    assert_eq!(
        binding_evidence(
            Some(&binding),
            &agent.id,
            &request,
            agent.persisted_launch_signature.as_ref(),
            durable,
        ),
        BindingEvidence::DefinitionDrift
    );

    binding.session_name = "jefe-agent-other".to_owned();
    assert_eq!(
        binding_evidence(
            Some(&binding),
            &agent.id,
            &request,
            agent.persisted_launch_signature.as_ref(),
            durable,
        ),
        BindingEvidence::Inconsistent
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

#[test]
fn unavailable_runtime_probe_is_recoverable_not_phantom_dead() {
    for liveness in [ProcessLiveness::Dead, ProcessLiveness::ProbeFailure] {
        assert_eq!(
            classify_startup(
                SessionEvidence::Unavailable,
                BindingEvidence::Coherent,
                false,
                liveness,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Recoverable
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
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::Coherent,
            false,
            ProcessLiveness::Inaccessible,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Recoverable
    );
}

#[test]
fn live_session_survives_definition_hash_drift() {
    for liveness in [ProcessLiveness::Alive, ProcessLiveness::MalformedIdentity] {
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::DefinitionDrift,
                false,
                liveness,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::DefinitionDrift,
            "live local session with definition drift must use reattach-only registration"
        );
    }
    assert_eq!(
        classify_startup(
            SessionEvidence::Alive,
            BindingEvidence::DefinitionDrift,
            false,
            ProcessLiveness::Dead,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
}

#[test]
fn definition_drift_does_not_override_reused_pid_or_missing_session() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Alive,
            BindingEvidence::DefinitionDrift,
            false,
            ProcessLiveness::ReusedPid,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Stale
    );
    assert_eq!(
        classify_startup(
            SessionEvidence::Missing,
            BindingEvidence::DefinitionDrift,
            false,
            ProcessLiveness::Alive,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
    );
}

#[test]
fn remote_definition_drift_is_rejected() {
    assert_eq!(
        classify_startup(
            SessionEvidence::Alive,
            BindingEvidence::DefinitionDrift,
            true,
            ProcessLiveness::MalformedIdentity,
            jefe::runtime::OrphanClassification::NoOrphan,
        ),
        StartupClassification::Inconsistent
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
