//! Launch-request tests for repository transient options (issue #317).

use super::{launch_signature_for_transient, transient_queue_ops::agent_from_queued_signature};
use jefe::domain::{AgentId, AgentTypeId, Repository, RepositoryId, TypedMap, TypedValue};
use tempfile::TempDir;

fn transient_repository(type_id: AgentTypeId) -> (TempDir, Repository) {
    let root =
        tempfile::tempdir().unwrap_or_else(|error| panic!("create temp repository: {error}"));
    let repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        type_id,
        TypedMap::new(),
        "Repo".to_owned(),
        "repo".to_owned(),
        root.path().join("repo"),
    );
    (root, repository)
}

fn set_value(values: &mut TypedMap, field: &str, value: serde_json::Value) {
    jefe::domain::canonical_values::insert_json(values, field, value)
        .unwrap_or_else(|error| panic!("valid {field} fixture: {error}"));
}

#[test]
fn transient_request_copies_repository_type_values_and_target() {
    let (_root, mut repository) = transient_repository(jefe::domain::shipped_agent_type(3));
    set_value(
        &mut repository.default_values,
        "profile",
        serde_json::Value::String("review".to_owned()),
    );
    set_value(
        &mut repository.default_values,
        "yolo",
        serde_json::Value::Bool(true),
    );
    let work_dir = repository.effective_transient_dir().join("transient");

    let request = launch_signature_for_transient(&repository, &work_dir);

    assert_eq!(request.type_id, repository.default_type_id);
    assert_eq!(request.values, repository.default_values);
    assert_eq!(request.work_dir, work_dir);
    assert_eq!(request.remote, repository.remote);
    assert_eq!(
        request.operation,
        jefe::domain::agent_definition::Operation::Normal
    );
}

#[test]
fn dequeued_agent_retains_the_queued_launch_snapshot() {
    let (_root, mut repository) = transient_repository(jefe::domain::shipped_agent_type(1));
    set_value(
        &mut repository.default_values,
        "model",
        serde_json::Value::String("queued-model".to_owned()),
    );
    set_value(
        &mut repository.default_values,
        "yolo",
        serde_json::Value::Bool(true),
    );
    let request = launch_signature_for_transient(
        &repository,
        &repository.effective_transient_dir().join("queued"),
    );

    repository.default_values.clear();

    let agent = agent_from_queued_signature(
        AgentId("transient-317".to_owned()),
        repository.id.clone(),
        &repository,
        &request,
    );

    assert_eq!(agent.type_id, request.type_id);
    assert_eq!(agent.values, request.values);
    assert_eq!(
        jefe::domain::canonical_values::typed_field(&agent.values, "model"),
        Some(&TypedValue::String("queued-model".to_owned()))
    );
    assert_eq!(
        jefe::domain::canonical_values::typed_field(&agent.values, "yolo"),
        Some(&TypedValue::Bool(true))
    );
}

#[test]
fn transient_request_preserves_declared_yolo_opt_out() {
    let (_root, mut repository) = transient_repository(jefe::domain::shipped_agent_type(1));
    set_value(
        &mut repository.default_values,
        "yolo",
        serde_json::Value::Bool(false),
    );
    let work_dir = repository.effective_transient_dir().join("transient");

    let request = launch_signature_for_transient(&repository, &work_dir);

    assert_eq!(
        jefe::domain::canonical_values::typed_field(&request.values, "yolo"),
        Some(&TypedValue::Bool(false))
    );
}
