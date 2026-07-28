//! Pure runtime-choice transitions shared by agent and repository forms.

use crate::domain::SandboxEngine;
use crate::domain::agent_definition::AgentTypeId;

use super::{AgentFormFields, AgentFormFocus};

pub(super) fn effective_agent_type_ids(
    available: &[AgentTypeId],
    is_remote: bool,
) -> Vec<AgentTypeId> {
    super::form_projection::effective_agent_type_ids(available, is_remote)
}

pub(super) fn cycle_agent_field(
    available: &[AgentTypeId],
    fields: &mut AgentFormFields,
    focus: AgentFormFocus,
    c: char,
) {
    if c != ' ' && c != 'x' && c != 'X' {
        return;
    }

    match focus {
        AgentFormFocus::AgentType => {
            if let Some(next) = next_available_type(available, &fields.agent_type_id) {
                next.as_str().clone_into(&mut fields.agent_type_id);
            }
        }
        AgentFormFocus::CodePuppyYolo => fields.code_puppy_yolo = !fields.code_puppy_yolo,
        AgentFormFocus::CodePuppyQuickResume => fields.code_puppy_quick_resume.toggle(),
        AgentFormFocus::PassContinue => fields.pass_continue = !fields.pass_continue,
        AgentFormFocus::Sandbox => fields.sandbox_enabled = !fields.sandbox_enabled,
        AgentFormFocus::SandboxEngine => {
            SandboxEngine::next_from_form_value(&fields.sandbox_engine)
                .label()
                .clone_into(&mut fields.sandbox_engine);
        }
        _ => {}
    }
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

pub(super) fn repository_slug_from_name(name: &str) -> String {
    slugify(name)
}

pub(super) fn derive_local_work_dir_from_name(name: &str, base_dir: &std::path::Path) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        base_dir.to_string_lossy().into_owned()
    } else {
        base_dir.join(slug).to_string_lossy().into_owned()
    }
}

pub(super) fn derive_remote_work_dir_from_name(name: &str, base_dir: &str) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        base_dir.to_owned()
    } else {
        format!("{}/{slug}", base_dir.trim_end_matches('/'))
    }
}

pub(super) fn next_available_type(available: &[AgentTypeId], value: &str) -> Option<AgentTypeId> {
    let current = super::form_projection::type_id_from_form_value(value);
    current
        .and_then(|current| available.iter().position(|type_id| *type_id == current))
        .map_or_else(
            || available.first().cloned(),
            |index| available.get((index + 1) % available.len()).cloned(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_work_dir_trims_trailing_separators_before_joining_slug() {
        assert_eq!(
            derive_remote_work_dir_from_name("Branch 1", "~/remote///"),
            "~/remote/branch-1"
        );
    }
}
