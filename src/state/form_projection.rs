//! Definition-driven projections shared by agent and repository forms.

use crate::domain::agent_definition::{AgentDefinition, AgentTypeId};

/// Resolve the effective agent-type choices for a form.
///
/// Remote repositories expose every shipped definition because local PATH
/// observations cannot describe a remote host. Local repositories expose only
/// enabled, compatible IDs captured by startup probing.
#[must_use]
pub fn effective_agent_type_ids(available: &[AgentTypeId], is_remote: bool) -> Vec<AgentTypeId> {
    if is_remote {
        AgentDefinition::shipped()
            .into_iter()
            .map(|definition| definition.id)
            .collect()
    } else {
        available.to_vec()
    }
}

/// Format effective agent types as a space-separated display-name hint.
#[must_use]
pub fn effective_types_hint(type_ids: &[AgentTypeId]) -> String {
    let definitions = AgentDefinition::shipped();
    let labels = type_ids
        .iter()
        .map(|type_id| {
            definitions
                .iter()
                .find(|definition| definition.id == *type_id)
                .map_or_else(
                    || type_id.as_str(),
                    |definition| definition.display_name.as_str(),
                )
        })
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "no available agents".to_owned()
    } else {
        format!("space cycles: {}", labels.join(" / "))
    }
}

/// Find a shipped definition by typed ID. Unknown IDs are never defaulted.
#[must_use]
pub fn definition_for_type(type_id: &AgentTypeId) -> Option<AgentDefinition> {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == *type_id)
}

/// Parse a form value as a known active type.
#[must_use]
pub fn type_id_from_form_value(value: &str) -> Option<AgentTypeId> {
    let parsed = AgentTypeId::parse(value.trim()).ok()?;
    definition_for_type(&parsed).map(|definition| definition.id)
}

/// Definition metadata needed by the temporary structural form shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFormField {
    Profile,
    Model,
    VersionSelector,
    Yolo,
    Interactive,
    PromptInteractive,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentFormFieldVisibility(Vec<AgentFormField>);

impl AgentFormFieldVisibility {
    fn contains(&self, field: AgentFormField) -> bool {
        self.0.contains(&field)
    }

    #[must_use]
    pub fn shows_profile_fields(&self) -> bool {
        self.contains(AgentFormField::Profile)
    }

    #[must_use]
    pub fn shows_model_fields(&self) -> bool {
        self.contains(AgentFormField::Model)
    }

    #[must_use]
    pub fn shows_llxprt_fields(&self) -> bool {
        self.shows_profile_fields() && self.contains(AgentFormField::PromptInteractive)
    }
}

/// Compute legacy-shell visibility strictly from definition field metadata.
#[must_use]
pub fn agent_form_visibility(type_id: Option<&AgentTypeId>) -> AgentFormFieldVisibility {
    let Some(type_id) = type_id else {
        return AgentFormFieldVisibility::default();
    };
    let Some(definition) = definition_for_type(type_id) else {
        return AgentFormFieldVisibility::default();
    };
    let has_repository = |id: &str| {
        definition
            .repository_fields
            .iter()
            .any(|field| field.id == id)
    };
    let has_agent = |id: &str| definition.agent_fields.iter().any(|field| field.id == id);
    AgentFormFieldVisibility(
        [
            has_repository("profile").then_some(AgentFormField::Profile),
            has_repository("model").then_some(AgentFormField::Model),
            has_agent("version_selector").then_some(AgentFormField::VersionSelector),
            has_repository("yolo").then_some(AgentFormField::Yolo),
            has_agent("interactive").then_some(AgentFormField::Interactive),
            has_agent("prompt_interactive").then_some(AgentFormField::PromptInteractive),
        ]
        .into_iter()
        .flatten()
        .collect(),
    )
}

#[must_use]
pub fn is_field_visible(
    focus: crate::state::AgentFormFocus,
    visibility: &AgentFormFieldVisibility,
) -> bool {
    use crate::state::AgentFormFocus as F;
    match focus {
        F::Profile | F::LlxprtDebug | F::Sandbox | F::SandboxEngine | F::SandboxFlags => {
            visibility.shows_profile_fields()
        }
        F::CodePuppyModel => visibility.shows_model_fields(),
        F::CodePuppyVersion => {
            visibility.shows_model_fields() && visibility.contains(AgentFormField::VersionSelector)
        }
        F::LlxprtVersion => {
            visibility.shows_profile_fields()
                && visibility.contains(AgentFormField::VersionSelector)
        }
        F::CodePuppyYolo => {
            visibility.shows_model_fields() && visibility.contains(AgentFormField::Yolo)
        }
        F::CodePuppyQuickResume => visibility.contains(AgentFormField::Interactive),
        F::Mode => visibility.shows_profile_fields() && visibility.contains(AgentFormField::Yolo),
        F::PassContinue => visibility.contains(AgentFormField::PromptInteractive),
        F::Shortcut | F::Name | F::Description | F::WorkDir | F::AgentType => true,
    }
}

#[must_use]
pub fn next_visible_focus(
    focus: crate::state::AgentFormFocus,
    visibility: &AgentFormFieldVisibility,
) -> crate::state::AgentFormFocus {
    let start = focus;
    let mut current = focus.next();
    while current != start {
        if is_field_visible(current, visibility) {
            return current;
        }
        current = current.next();
    }
    start
}

#[must_use]
pub fn prev_visible_focus(
    focus: crate::state::AgentFormFocus,
    visibility: &AgentFormFieldVisibility,
) -> crate::state::AgentFormFocus {
    let start = focus;
    let mut current = focus.prev();
    while current != start {
        if is_field_visible(current, visibility) {
            return current;
        }
        current = current.prev();
    }
    start
}

#[must_use]
pub fn is_repository_field_visible(
    focus: crate::state::RepositoryFormFocus,
    type_id: Option<&AgentTypeId>,
) -> bool {
    use crate::state::RepositoryFormFocus as F;
    let visibility = agent_form_visibility(type_id);
    match focus {
        F::DefaultProfile => visibility.shows_profile_fields(),
        F::DefaultCodePuppyModel => visibility.shows_model_fields(),
        F::DefaultCodePuppyYolo => {
            visibility.shows_model_fields() && visibility.contains(AgentFormField::Yolo)
        }
        F::DefaultCodePuppyVersion => {
            visibility.shows_model_fields() && visibility.contains(AgentFormField::VersionSelector)
        }
        F::DefaultLlxprtMode => {
            visibility.shows_profile_fields() && visibility.contains(AgentFormField::Yolo)
        }
        F::DefaultLlxprtVersion => {
            visibility.shows_profile_fields()
                && visibility.contains(AgentFormField::VersionSelector)
        }
        _ => true,
    }
}

#[must_use]
pub fn next_visible_repository_focus(
    focus: crate::state::RepositoryFormFocus,
    type_id: &AgentTypeId,
) -> crate::state::RepositoryFormFocus {
    let start = focus;
    let mut current = focus.next();
    while current != start {
        if is_repository_field_visible(current, Some(type_id)) {
            return current;
        }
        current = current.next();
    }
    start
}

#[must_use]
pub fn prev_visible_repository_focus(
    focus: crate::state::RepositoryFormFocus,
    type_id: &AgentTypeId,
) -> crate::state::RepositoryFormFocus {
    let start = focus;
    let mut current = focus.prev();
    while current != start {
        if is_repository_field_visible(current, Some(type_id)) {
            return current;
        }
        current = current.prev();
    }
    start
}
