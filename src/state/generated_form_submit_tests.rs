//! Focused unit tests for the generated agent form submit path (issue #382 S6).

use super::*;
use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::agent_definition::{
    AgentDefinition, AgentTypeId, Availability, FieldValue, Operation,
};
use crate::domain::{Id, Repository, RepositoryId, TypedMap, TypedValue};
use crate::state::generated_agent_form::{
    GeneratedAgentForm, GeneratedAgentFormFocus, GeneratedAgentFormIntent,
    GeneratedAgentFormResult, GeneratedTarget,
};
use crate::state::generated_form::FormFieldValue;
use crate::state::transition::TransitionExt;
use std::path::PathBuf;

fn llxprt_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"))
}

fn compatible_with_prompt_interactive() -> Availability {
    Availability::InstalledCompatible {
        identity: "0.10.0".to_string(),
        capabilities: vec!["prompt-interactive".to_string()],
        generation: 1,
    }
}

fn test_repository() -> Repository {
    Repository::new(
        RepositoryId("test-repo".to_string()),
        llxprt_definition().id,
        TypedMap::new(),
        "Test Repo".to_string(),
        "test-repo".to_string(),
        PathBuf::from("/tmp/jefe-test-repo"),
    )
}

fn build_form_for_llxprt() -> GeneratedAgentForm {
    let definition = llxprt_definition();
    let availability = compatible_with_prompt_interactive();
    GeneratedAgentForm::from_definition(&definition, &availability)
        .unwrap_or_else(|error| panic!("LLxprt definition must produce a form: {error}"))
}

fn form_result_with_values(values: Vec<FieldValue>) -> GeneratedAgentFormResult {
    GeneratedAgentFormResult {
        operation: Operation::Resume,
        target: GeneratedTarget::Local,
        values: vec![
            FormFieldValue {
                id: crate::state::generated_form::FormFieldId::repository("profile"),
                value: values[0].clone(),
            },
            FormFieldValue {
                id: crate::state::generated_form::FormFieldId::repository("yolo"),
                value: values[1].clone(),
            },
            FormFieldValue {
                id: crate::state::generated_form::FormFieldId::agent("version_selector"),
                value: values[2].clone(),
            },
            FormFieldValue {
                id: crate::state::generated_form::FormFieldId::agent("prompt"),
                value: values[3].clone(),
            },
            FormFieldValue {
                id: crate::state::generated_form::FormFieldId::agent("prompt_interactive"),
                value: values[4].clone(),
            },
        ],
    }
}

#[test]
fn values_from_form_result_normalizes_underscores_to_hyphens() {
    let definition = llxprt_definition();
    let result = form_result_with_values(vec![
        FieldValue::String("nightly".to_string()),
        FieldValue::Boolean(true),
        FieldValue::String("latest".to_string()),
        FieldValue::String("do the thing".to_string()),
        FieldValue::Boolean(true),
    ]);
    let map = values_from_form_result(&definition, &result.values);

    // Underscored field IDs must normalize to hyphenated typed IDs.
    let profile =
        Id::parse("profile").unwrap_or_else(|error| panic!("profile is a valid Id: {error}"));
    assert_eq!(
        map.get(&profile),
        Some(&TypedValue::String("nightly".to_string()))
    );

    let yolo = Id::parse("yolo").unwrap_or_else(|error| panic!("yolo is a valid Id: {error}"));
    assert_eq!(map.get(&yolo), Some(&TypedValue::Bool(true)));

    let version_selector = Id::parse("version-selector")
        .unwrap_or_else(|error| panic!("version-selector is a valid Id: {error}"));
    assert_eq!(
        map.get(&version_selector),
        Some(&TypedValue::String("latest".to_string()))
    );

    let prompt =
        Id::parse("prompt").unwrap_or_else(|error| panic!("prompt is a valid Id: {error}"));
    assert_eq!(
        map.get(&prompt),
        Some(&TypedValue::String("do the thing".to_string()))
    );

    let prompt_interactive = Id::parse("prompt-interactive")
        .unwrap_or_else(|error| panic!("prompt-interactive is a valid Id: {error}"));
    assert_eq!(map.get(&prompt_interactive), Some(&TypedValue::Bool(true)));
}

#[test]
fn field_value_to_typed_converts_all_variants() {
    assert_eq!(
        field_value_to_typed(FieldValue::Boolean(true)),
        Some(TypedValue::Bool(true))
    );
    assert_eq!(
        field_value_to_typed(FieldValue::OptionalBoolean(None)),
        None
    );
    assert_eq!(
        field_value_to_typed(FieldValue::OptionalBoolean(Some(true))),
        Some(TypedValue::Bool(true))
    );
    assert_eq!(
        field_value_to_typed(FieldValue::String("hi".to_string())),
        Some(TypedValue::String("hi".to_string()))
    );
    assert_eq!(
        field_value_to_typed(FieldValue::Path("/tmp".to_string())),
        Some(TypedValue::String("/tmp".to_string()))
    );
    assert_eq!(
        field_value_to_typed(FieldValue::Integer(42)),
        Some(TypedValue::Integer(42))
    );
    assert_eq!(
        field_value_to_typed(FieldValue::StringList(vec![
            "a".to_string(),
            "b".to_string()
        ])),
        Some(TypedValue::List(vec![
            TypedValue::String("a".to_string()),
            TypedValue::String("b".to_string())
        ]))
    );
}

#[test]
fn derive_work_dir_local_joins_base_dir_with_slug() {
    let repo = test_repository();
    let dir = derive_work_dir(&repo, GeneratedTarget::Local, "LLxprt");
    assert_eq!(
        std::path::PathBuf::from(dir),
        std::path::Path::new("/tmp/jefe-test-repo").join("llxprt")
    );
}

#[test]
fn derive_work_dir_remote_reuses_repository_base_dir() {
    let repo = test_repository();
    let dir = derive_work_dir(&repo, GeneratedTarget::Remote, "LLxprt");
    assert_eq!(dir, "/tmp/jefe-test-repo");
}

#[test]
fn build_generated_agent_produces_named_agent_with_type_id_and_typed_values() {
    let repo = test_repository();
    let type_id = llxprt_definition().id;
    let result = form_result_with_values(vec![
        FieldValue::String("nightly".to_string()),
        FieldValue::Boolean(true),
        FieldValue::String("latest".to_string()),
        FieldValue::String("do the thing".to_string()),
        FieldValue::Boolean(true),
    ]);
    let Some(agent) = build_generated_agent(&repo, &type_id, &result, 1) else {
        panic!("must build an agent for a valid definition and repository");
    };

    assert_eq!(agent.name, "LLxprt");
    assert_eq!(agent.type_id, type_id);
    assert_eq!(agent.repository_id, repo.id);
    assert_eq!(agent.status, crate::domain::AgentStatus::Queued);

    // TypedMap carried through the canonical normalization.
    let profile =
        Id::parse("profile").unwrap_or_else(|error| panic!("profile is a valid Id: {error}"));
    assert_eq!(
        agent.values.get(&profile),
        Some(&TypedValue::String("nightly".to_string()))
    );
}

#[test]
fn build_generated_agent_rejects_unknown_type_id() {
    let repo = test_repository();
    let unknown = AgentTypeId::from_validated("core.does-not-exist");
    let result = form_result_with_values(vec![
        FieldValue::String(String::new()),
        FieldValue::Boolean(false),
        FieldValue::String(String::new()),
        FieldValue::String(String::new()),
        FieldValue::Boolean(false),
    ]);
    assert!(build_generated_agent(&repo, &unknown, &result, 1).is_none());
}

#[test]
fn form_create_enabled_when_operation_target_and_values_valid() {
    let mut form = build_form_for_llxprt();
    // LLxprt supports all operations when the required capability is present.
    assert!(form.create_enabled());

    // The validated result should be available after activating Create.
    form.apply(GeneratedAgentFormIntent::Activate);
    // Focus starts on Operation(Resume), so Activate selects it — no result yet.
    // Navigate to Create and activate.
    while !matches!(form.focus(), GeneratedAgentFormFocus::Create) {
        form.apply(GeneratedAgentFormIntent::Next);
    }
    form.apply(GeneratedAgentFormIntent::Activate);
    assert!(form.validated_result().is_some());
    let Some(result) = form.take_validated_result() else {
        panic!("result consumed once");
    };
    assert_eq!(result.operation, Operation::Resume);
    assert_eq!(result.target, GeneratedTarget::Local);
}

#[test]
fn take_validated_result_consumes_exactly_once() {
    let mut form = build_form_for_llxprt();
    while !matches!(form.focus(), GeneratedAgentFormFocus::Create) {
        form.apply(GeneratedAgentFormIntent::Next);
    }
    form.apply(GeneratedAgentFormIntent::Activate);
    assert!(
        form.take_validated_result().is_some(),
        "first take succeeds"
    );
    assert!(
        form.take_validated_result().is_none(),
        "second take returns None — consumed exactly once"
    );
}

#[test]
fn slugify_matches_form_runtime_behavior() {
    assert_eq!(slugify("LLxprt"), "llxprt");
    assert_eq!(slugify("Claude Code"), "claude-code");
}

#[test]
fn agent_type_id_is_preserved_in_modal_state() {
    // Verifies the type_id field on ModalState::GeneratedAgent — the selected
    // definition/type ID is the sole authority for the canonical path.
    let type_id = llxprt_definition().id;
    let availability = compatible_with_prompt_interactive();
    let observation = AgentAvailabilityObservation::new(&llxprt_definition(), true, availability);
    let mut state = crate::state::AppState {
        agent_type_availability: vec![observation],
        ..crate::state::AppState::default()
    };
    state = state
        .apply(crate::state::AppEvent::OpenAgentTypeForm(type_id.clone()))
        .committed_pure();
    match state.modal {
        crate::state::ModalState::GeneratedAgent {
            type_id: modal_type_id,
            ..
        } => {
            assert_eq!(*modal_type_id, type_id);
        }
        _ => panic!("expected GeneratedAgent modal"),
    }
}
