//! Canonical production submit path for the definition-driven generated agent
//! form (issue #382 S6).
//!
//! Consumes exactly one [`GeneratedAgentFormResult`], converts the typed
//! `FormFieldValue` list into a normalized [`TypedMap`] (definition/type ID
//! and TypedMap are the sole authority), and routes through canonical generic
//! `Repository`/`Agent` state creation. This reducer stays deterministic;
//! filesystem and runtime effects belong to app-input launch orchestration.
//! Unsupported or invalid Create leaves state untouched.

use crate::domain::agent_definition::FieldValue;
use crate::domain::{Agent, AgentId, Id, Repository, TypedMap, TypedValue};
use crate::services::generate_id;

use super::AppState;
use super::generated_agent_form::{GeneratedAgentFormResult, GeneratedTarget};
use super::generated_form::FormFieldValue;
use super::types::ModalState;

impl AppState {
    /// Consume one validated generated-form result and create the agent through
    /// the canonical path. No-op (zero effects) when the result is absent, the
    /// selected repository is missing, or agent construction fails.
    pub(super) fn consume_generated_form_result(&mut self) {
        let (return_focus, return_agent_type_index) = {
            let ModalState::GeneratedAgent {
                return_focus,
                return_agent_type_index,
                ..
            } = &mut self.modal
            else {
                return;
            };
            (*return_focus, *return_agent_type_index)
        };

        let Some(agent) = self.build_generated_agent_from_modal() else {
            return;
        };

        self.agents.push(agent);
        self.selected_agent_index = Some(self.agents.len() - 1);
        self.remember_selected_agent_for_current_repo();

        self.pane_focus = return_focus;
        self.selected_agent_type_index = return_agent_type_index;
        self.modal = ModalState::None;
    }

    fn build_generated_agent_from_modal(&mut self) -> Option<Agent> {
        let (type_id, result) = self.take_generated_form_result()?;
        let repository = self.selected_repository().cloned()?;
        let next_display_index = self.agents.len() + 1;
        build_generated_agent(&repository, &type_id, &result, next_display_index)
    }

    fn take_generated_form_result(
        &mut self,
    ) -> Option<(
        crate::domain::agent_definition::AgentTypeId,
        GeneratedAgentFormResult,
    )> {
        let ModalState::GeneratedAgent { type_id, form, .. } = &mut self.modal else {
            return None;
        };
        form.take_validated_result()
            .map(|result| ((**type_id).clone(), result))
    }
}

/// Construct one agent from the canonical inputs, applying side effects
/// (local work-directory creation) at this boundary only.
fn build_generated_agent(
    repository: &Repository,
    type_id: &crate::domain::agent_definition::AgentTypeId,
    result: &GeneratedAgentFormResult,
    next_display_index: usize,
) -> Option<Agent> {
    let definition = crate::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == *type_id)?;
    let values = values_from_form_result(&definition, &result.values);
    let name = definition.display_name.clone();
    let work_dir = derive_work_dir(repository, result.target, &name);
    let mut agent = Agent::new(
        AgentId(generate_id("agent")),
        repository.id.clone(),
        type_id.clone(),
        values,
        name,
        std::path::PathBuf::from(work_dir.clone()),
    );
    agent.display_id = format!("#{next_display_index}");
    Some(agent)
}

/// Convert the active form values into a TypedMap keyed by normalized typed IDs
/// (`_` replaced with `-`). Only declared definition fields are retained so the
/// definition remains the sole authority over the value map.
fn values_from_form_result(
    definition: &crate::domain::agent_definition::AgentDefinition,
    values: &[FormFieldValue],
) -> TypedMap {
    let mut map = default_values_for_definition(definition);
    let declared_ids: Vec<&str> = definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
        .map(|field| field.id.as_str())
        .collect();
    for value in values {
        let raw_id = value.id().as_str();
        if !declared_ids.contains(&raw_id) {
            continue;
        }
        let Ok(key) = Id::parse(&raw_id.replace('_', "-")) else {
            continue;
        };
        if let Some(value) = field_value_to_typed(value.value().clone()) {
            map.insert(key, value);
        } else {
            map.remove(&key);
        }
    }
    map
}

/// Build the declared-default TypedMap for one definition so unspecified
/// optional fields keep their authored baseline.
fn default_values_for_definition(
    definition: &crate::domain::agent_definition::AgentDefinition,
) -> TypedMap {
    let mut values = TypedMap::new();
    for field in definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
    {
        let Some(default) = &field.default else {
            continue;
        };
        let Ok(key) = Id::parse(&field.id.replace('_', "-")) else {
            continue;
        };
        if let Some(default) = field_value_to_typed(default.clone()) {
            values.insert(key, default);
        }
    }
    values
}

/// Resolve the work directory for the selected target without inventing remote
/// settings. Local targets derive from the repository base dir; remote targets
/// reuse the repository's configured base dir verbatim (the remote host owns
/// path resolution).
fn derive_work_dir(repository: &Repository, target: GeneratedTarget, name: &str) -> String {
    match target {
        GeneratedTarget::Local => {
            let slug = slugify(name);
            if slug.is_empty() {
                repository.base_dir.to_string_lossy().into_owned()
            } else {
                repository
                    .base_dir
                    .join(slug)
                    .to_string_lossy()
                    .into_owned()
            }
        }
        GeneratedTarget::Remote => repository.base_dir.to_string_lossy().into_owned(),
    }
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Convert one closed-schema value, preserving optional absence by omission.
fn field_value_to_typed(value: FieldValue) -> Option<TypedValue> {
    match value {
        FieldValue::Boolean(value) => Some(TypedValue::Bool(value)),
        FieldValue::OptionalBoolean(value) => value.map(TypedValue::Bool),
        FieldValue::String(value) | FieldValue::Path(value) => Some(TypedValue::String(value)),
        FieldValue::Integer(value) => Some(TypedValue::Integer(value)),
        FieldValue::StringList(values) => Some(TypedValue::List(
            values.into_iter().map(TypedValue::String).collect(),
        )),
    }
}

#[cfg(test)]
#[path = "generated_form_submit_tests.rs"]
mod tests;
