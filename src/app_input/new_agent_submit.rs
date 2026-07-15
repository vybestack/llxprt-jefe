//! Pre-submit planning for New Agent package availability validation.

use jefe::domain::{LaunchSignature, llxprt_launch_source};
use jefe::services::{CreateAgentParams, prospective_agent_launch};
use jefe::state::{AppEvent, AppState, ModalState};

pub(super) enum NewAgentPackageProbePlan {
    Probe(Box<LaunchSignature>),
    Skip,
    Invalid(String),
}

/// Build the selector-backed launch target represented by the current New
/// Agent draft without applying the reducer or touching the filesystem.
#[must_use]
pub(super) fn new_agent_package_probe_plan(state: &AppState) -> NewAgentPackageProbePlan {
    let ModalState::NewAgent {
        repository_id,
        fields,
        ..
    } = &state.modal
    else {
        return NewAgentPackageProbePlan::Skip;
    };
    let Some(repository) = state.repository_by_id(repository_id) else {
        return NewAgentPackageProbePlan::Invalid("selected repository is missing".to_owned());
    };
    let Some(signature) = prospective_agent_launch(&CreateAgentParams {
        repository,
        name: &fields.name,
        description: &fields.description,
        work_dir: &fields.work_dir,
        profile: &fields.profile,
        code_puppy_model: &fields.code_puppy_model,
        code_puppy_yolo: fields.code_puppy_yolo,
        code_puppy_quick_resume: fields.code_puppy_quick_resume,
        agent_kind: &fields.agent_kind,
        llxprt_version: &fields.llxprt_version,
        mode: &fields.mode,
        llxprt_debug: &fields.llxprt_debug,
        pass_continue: fields.pass_continue,
        sandbox_enabled: fields.sandbox_enabled,
        sandbox_engine: &fields.sandbox_engine,
        sandbox_flags: &fields.sandbox_flags,
        shortcut_slot: fields.shortcut_slot,
        next_display_index: state.agents.len() + 1,
    }) else {
        return NewAgentPackageProbePlan::Invalid(
            "invalid new-agent launch configuration".to_owned(),
        );
    };
    if llxprt_launch_source(signature.agent_kind, signature.llxprt_version.as_ref()).requires_npm()
    {
        NewAgentPackageProbePlan::Probe(Box::new(signature))
    } else {
        NewAgentPackageProbePlan::Skip
    }
}

/// Execute the package boundary through an injectable seam. A missing plan is
/// deliberately probe-free for repository forms, Edit Agent, and direct launches.
pub(super) fn execute_new_agent_package_probe<F, E>(
    plan: &NewAgentPackageProbePlan,
    probe: F,
) -> Result<(), String>
where
    F: FnOnce(&LaunchSignature) -> Result<(), E>,
    E: std::fmt::Display,
{
    match plan {
        NewAgentPackageProbePlan::Probe(signature) => {
            probe(signature).map_err(|error| error.to_string())
        }
        NewAgentPackageProbePlan::Skip => Ok(()),
        NewAgentPackageProbePlan::Invalid(error) => Err(error.clone()),
    }
}

/// Apply SubmitForm only after the package boundary accepts the prospective
/// launch. Rejection mutates only the visible error and leaves the draft intact.
pub(super) fn apply_form_submit_after_package_probe(
    state: &mut AppState,
    probe_result: Result<(), String>,
) -> bool {
    match probe_result {
        Ok(()) => {
            *state = std::mem::take(state).apply(AppEvent::SubmitForm);
            true
        }
        Err(error) => {
            state.error_message = Some(error);
            false
        }
    }
}
