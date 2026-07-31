use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use jefe::domain::{AgentId, AgentLaunchRequest, Repository, RepositoryId};
use jefe::runtime::NpmPackageAvailabilityError;
use jefe::state::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus,
};

use super::launch_signature_for_new_agent;
use super::new_agent_submit::{
    apply_form_submit_after_package_probe, execute_new_agent_package_probe,
    new_agent_package_probe_plan,
};

fn new_agent_state(root: &Path) -> (AppState, PathBuf) {
    let repository = Repository::new(
        RepositoryId("repo-package-probe".to_owned()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Package Probe".to_owned(),
        "package-probe".to_owned(),
        root.join("repository"),
    );
    let work_dir = root.join("prospective-agent");
    let modal = ModalState::NewAgent {
        repository_id: repository.id.clone(),
        fields: AgentFormFields {
            name: "Prospective Agent".to_owned(),
            work_dir: work_dir.to_string_lossy().into_owned(),
            agent_type_id: "core.llxprt".to_owned(),
            llxprt_version: " nightly ".to_owned(),
            ..AgentFormFields::default()
        },
        focus: AgentFormFocus::Name,
        cursor: AgentFormCursor::default(),
        work_dir_manual: true,
    };
    (
        AppState {
            repositories: vec![repository],
            selected_repository_index: Some(0),
            modal,
            available_agent_type_ids: vec![jefe::domain::shipped_agent_type(3)],
            ..AppState::default()
        },
        work_dir,
    )
}

fn submit_with_probe<F>(state: &mut AppState, probe: F) -> bool
where
    F: FnOnce(&AgentLaunchRequest) -> Result<(), NpmPackageAvailabilityError>,
{
    let plan = new_agent_package_probe_plan(state);
    let result = execute_new_agent_package_probe(&plan, probe);
    apply_form_submit_after_package_probe(state, result)
}

fn probe_failures() -> Vec<NpmPackageAvailabilityError> {
    vec![
        NpmPackageAvailabilityError::NpmMissing {
            target: "local machine".to_owned(),
            selector: "nightly".to_owned(),
        },
        NpmPackageAvailabilityError::PackageUnresolved {
            target: "local machine".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "not found".to_owned(),
        },
        NpmPackageAvailabilityError::TransportFailure {
            target: "builder@example.test".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "connection refused".to_owned(),
        },
    ]
}

#[test]
fn package_probe_failure_rejects_submit_without_state_filesystem_or_follow_up_side_effects() {
    for failure in probe_failures() {
        let temp = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
        let (mut state, work_dir) = new_agent_state(temp.path());
        let original_modal = state.modal.clone();
        let expected_error = failure.to_string();
        let submitted = submit_with_probe(&mut state, |_| Err(failure));

        assert!(!submitted, "failed package probe must stop submission");
        assert_eq!(state.modal, original_modal, "modal draft must remain open");
        assert!(state.agents.is_empty(), "no agent may be inserted");
        assert!(!work_dir.exists(), "work directory must not be created");
        assert_eq!(
            state.error_message.as_deref(),
            Some(expected_error.as_str())
        );
        assert!(expected_error.contains("nightly"));
    }
}

#[test]
fn successful_package_probe_uses_prospective_signature_and_submit_proceeds() {
    let temp = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
    let (mut state, work_dir) = new_agent_state(temp.path());
    let observed_signature = RefCell::new(None);

    let submitted = submit_with_probe(&mut state, |signature| {
        observed_signature.replace(Some(signature.clone()));
        Ok(())
    });

    assert!(submitted, "successful probe must allow submission");
    assert_eq!(state.modal, ModalState::None);
    assert_eq!(state.agents.len(), 1);
    assert!(
        work_dir.is_dir(),
        "successful submit must create work directory"
    );
    let agent = &state.agents[0];
    let repository = &state.repositories[0];
    let observed = observed_signature.borrow();
    let observed = observed
        .as_ref()
        .unwrap_or_else(|| panic!("probe must observe a launch request"));
    let actual = launch_signature_for_new_agent(agent, repository);
    assert_eq!(observed.type_id, actual.type_id);
    assert_eq!(observed.work_dir, actual.work_dir);
    assert_eq!(observed.remote, actual.remote);
    assert_eq!(
        actual.operation,
        jefe::domain::agent_definition::Operation::Normal
    );
    assert_eq!(observed.operation, actual.operation);
    for (key, value) in &observed.values {
        assert_eq!(actual.values.get(key), Some(value));
    }
}

#[test]
fn pinned_code_puppy_new_agent_plans_exact_selected_package_probe() {
    let temp = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
    let (mut state, _) = new_agent_state(temp.path());
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("new-agent modal should be open");
    };
    fields.agent_type_id = jefe::domain::shipped_agent_type(1).to_string();
    fields.llxprt_version.clear();
    fields.code_puppy_version = "  0.0.361  ".to_owned();

    let plan = new_agent_package_probe_plan(&state);
    let called = Cell::new(false);
    let result = execute_new_agent_package_probe(&plan, |signature| {
        called.set(true);
        assert_eq!(signature.type_id, jefe::domain::shipped_agent_type(1));
        assert!(matches!(
            jefe::domain::canonical_values::typed_field(&signature.values, "version_selector"),
            Some(jefe::domain::TypedValue::String(value)) if value == "0.0.361"
        ));
        Ok::<(), &'static str>(())
    });
    assert!(result.is_ok());
    assert!(
        called.get(),
        "pinned Code Puppy must execute a package probe"
    );
}

#[test]
fn repository_and_edit_agent_forms_do_not_execute_package_probe() {
    let probe_calls = Cell::new(0);
    let mut states = Vec::new();
    states.push(AppState {
        modal: ModalState::NewRepository {
            fields: RepositoryFormFields::default(),
            focus: RepositoryFormFocus::Name,
            cursor: RepositoryFormCursor::default(),
        },
        ..AppState::default()
    });
    states.push(AppState {
        modal: ModalState::EditRepository {
            id: RepositoryId("repo-edit".to_owned()),
            fields: RepositoryFormFields::default(),
            focus: RepositoryFormFocus::Name,
            cursor: RepositoryFormCursor::default(),
        },
        ..AppState::default()
    });
    states.push(AppState {
        modal: ModalState::EditAgent {
            id: AgentId("agent-edit".to_owned()),
            fields: AgentFormFields {
                llxprt_version: "nightly".to_owned(),
                ..AgentFormFields::default()
            },
            focus: AgentFormFocus::LlxprtVersion,
            cursor: AgentFormCursor::default(),
        },
        ..AppState::default()
    });

    for state in &mut states {
        let plan = new_agent_package_probe_plan(state);
        let result = execute_new_agent_package_probe(&plan, |_| {
            probe_calls.set(probe_calls.get() + 1);
            Ok::<(), NpmPackageAvailabilityError>(())
        });
        assert!(result.is_ok());
    }

    assert_eq!(
        probe_calls.get(),
        0,
        "non-New-Agent forms must remain probe-free"
    );
}

#[test]
fn direct_new_agent_launch_does_not_execute_package_probe() {
    let temp = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
    let (mut state, _) = new_agent_state(temp.path());
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("test setup must contain NewAgent modal");
    };
    fields.llxprt_version.clear();
    let probe_calls = Cell::new(0);

    let plan = new_agent_package_probe_plan(&state);
    let result = execute_new_agent_package_probe(&plan, |_| {
        probe_calls.set(probe_calls.get() + 1);
        Ok::<(), NpmPackageAvailabilityError>(())
    });

    assert!(result.is_ok());
    assert_eq!(probe_calls.get(), 0);
}
