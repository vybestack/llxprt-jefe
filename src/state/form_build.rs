//! Repository and agent form-to-domain construction with validation.

use crate::domain::agent_definition::{AgentDefinition, AgentTypeId, FieldKind, FieldValue};
use crate::domain::{
    Agent, AgentId, AgentStatus, Id, RemoteRepositorySettings, Repository, RepositoryId, TypedMap,
    TypedValue, is_valid_github_component,
};
use tracing::warn;

use crate::services::{expand_tilde, generate_id, resolve_agent_work_dir};

use super::AppState;
use super::form_runtime;
use super::types::{AgentFormFields, RepositoryFormFields};

impl AppState {
    pub(super) fn validate_github_repo(value: &str) -> bool {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return true;
        }
        match trimmed.split_once('/') {
            Some((owner, repo)) => {
                !owner.is_empty()
                    && !repo.is_empty()
                    && !repo.contains('/')
                    && is_valid_github_component(owner)
                    && is_valid_github_component(repo)
            }
            None => false,
        }
    }

    pub(super) fn validated_agent_work_dir(repository: &Repository, value: &str) -> Option<String> {
        resolve_agent_work_dir(repository, value)
    }

    pub fn remote_settings_from_fields(
        fields: &RepositoryFormFields,
    ) -> Result<RemoteRepositorySettings, String> {
        let port = if fields.remote_enabled {
            match fields.ssh_port.trim() {
                "" => None,
                value => {
                    let port = value
                        .parse::<u16>()
                        .map_err(|_| "SSH port must be between 1 and 65535".to_owned())?;
                    if port == 0 {
                        return Err("SSH port must be between 1 and 65535".to_owned());
                    }
                    Some(port)
                }
            }
        } else {
            None
        };
        let settings = RemoteRepositorySettings {
            enabled: fields.remote_enabled,
            login_user: fields.login_user.trim().to_owned(),
            host: fields.host.trim().to_owned(),
            port,
            identity_file: std::path::PathBuf::from(fields.identity_file.trim()),
            options: fields
                .ssh_options
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            run_as_user: fields.run_as_user.trim().to_owned(),
            setup_env_default: fields.setup_env_default,
        };
        crate::domain::target::validate_remote(&settings)?;
        Ok(settings)
    }

    fn validated_remote_settings(
        fields: &RepositoryFormFields,
    ) -> Option<RemoteRepositorySettings> {
        match Self::remote_settings_from_fields(fields) {
            Ok(settings) => Some(settings),
            Err(error) => {
                warn!(error = %error, "rejecting repository create: invalid remote config");
                None
            }
        }
    }

    fn validate_repository_fields(
        fields: &RepositoryFormFields,
    ) -> Option<(String, String, RemoteRepositorySettings)> {
        let trimmed_name = fields.name.trim();
        if trimmed_name.is_empty() {
            return None;
        }
        let slug = form_runtime::repository_slug_from_name(trimmed_name);
        if slug.is_empty() || !Self::validate_github_repo(&fields.github_repo) {
            return None;
        }
        if let Err(error) = crate::domain::GitHubRepoRef::parse(&fields.github_issue_pr_repo) {
            warn!(
                github_issue_pr_repo = %fields.github_issue_pr_repo,
                error = %error,
                "rejecting repository: github_issue_pr_repo must be 'owner/repo' or empty"
            );
            return None;
        }
        let remote_settings = Self::validated_remote_settings(fields)?;
        Some((trimmed_name.to_owned(), slug, remote_settings))
    }

    pub(super) fn create_repository_from_fields(
        fields: &RepositoryFormFields,
    ) -> Option<Repository> {
        let (trimmed_name, slug, remote_settings) = Self::validate_repository_fields(fields)?;
        let type_id = active_type_id(&fields.default_type_id)?;
        let definition = definition_for_type(&type_id)?;
        let default_values = repository_values_from_fields(&definition, fields);
        let trimmed_base_dir = fields.base_dir.trim();
        let base_dir = if trimmed_base_dir.is_empty() {
            format!("/tmp/{slug}")
        } else if fields.remote_enabled {
            trimmed_base_dir.to_owned()
        } else {
            expand_tilde(trimmed_base_dir)
        };
        if !fields.remote_enabled
            && let Err(error) = std::fs::create_dir_all(&base_dir)
        {
            warn!(base_dir = %base_dir, error = %error, "could not create local repository base directory");
        }
        let mut repository = Repository::new(
            RepositoryId(generate_id("repo")),
            type_id,
            default_values,
            trimmed_name,
            slug,
            std::path::PathBuf::from(base_dir),
        );
        fields
            .github_repo
            .trim()
            .clone_into(&mut repository.github_repo);
        fields
            .github_issue_pr_repo
            .trim()
            .clone_into(&mut repository.github_issue_pr_repo);
        repository.remote = remote_settings;
        repository.transient_agent_dir = parse_transient_agent_dir(&fields.transient_agent_dir);
        repository.transient_max_concurrent =
            parse_transient_max_concurrent(&fields.transient_max_concurrent);
        Some(repository)
    }

    pub(super) fn update_repository_from_fields(
        repository: &mut Repository,
        fields: &RepositoryFormFields,
    ) -> bool {
        let Some((trimmed_name, slug, remote_settings)) = Self::validate_repository_fields(fields)
        else {
            return false;
        };
        let Some(type_id) = active_type_id(&fields.default_type_id) else {
            return false;
        };
        let Some(definition) = definition_for_type(&type_id) else {
            return false;
        };
        repository.name = trimmed_name;
        repository.slug = slug;
        let trimmed_base_dir = fields.base_dir.trim();
        if !trimmed_base_dir.is_empty() {
            repository.base_dir = if fields.remote_enabled {
                std::path::PathBuf::from(trimmed_base_dir)
            } else {
                std::path::PathBuf::from(expand_tilde(trimmed_base_dir))
            };
        }
        repository.default_type_id = type_id;
        repository.default_values = repository_values_from_fields(&definition, fields);
        fields
            .github_repo
            .trim()
            .clone_into(&mut repository.github_repo);
        fields
            .github_issue_pr_repo
            .trim()
            .clone_into(&mut repository.github_issue_pr_repo);
        repository.remote = remote_settings;
        repository.transient_agent_dir = parse_transient_agent_dir(&fields.transient_agent_dir);
        repository.transient_max_concurrent =
            parse_transient_max_concurrent(&fields.transient_max_concurrent);
        true
    }

    pub(super) fn create_agent_from_fields(
        repository: &Repository,
        fields: &AgentFormFields,
        next_display_index: usize,
    ) -> Option<Agent> {
        let type_id = active_type_id(&fields.agent_type_id)?;
        let definition = definition_for_type(&type_id)?;
        let work_dir = Self::validated_agent_work_dir(repository, &fields.work_dir)?;
        let mut values = if repository.default_type_id == type_id {
            repository.default_values.clone()
        } else {
            default_values(&definition)
        };
        merge_agent_values(&definition, fields, &mut values);
        let mut agent = Agent::new(
            AgentId(generate_id("agent")),
            repository.id.clone(),
            type_id,
            values,
            fields.name.trim().to_owned(),
            std::path::PathBuf::from(work_dir),
        );
        agent.display_id = format!("#{next_display_index}");
        agent.shortcut_slot = fields.shortcut_slot;
        agent.description.clone_from(&fields.description);
        agent.status = AgentStatus::Running;
        if !repository.remote.enabled
            && let Err(error) = std::fs::create_dir_all(&agent.work_dir)
        {
            warn!(work_dir = %agent.work_dir.display(), error = %error, "could not create local agent work directory");
        }
        Some(agent)
    }

    pub(super) fn update_agent_from_fields(
        agent: &mut Agent,
        repository: &Repository,
        fields: &AgentFormFields,
    ) {
        let Some(type_id) = active_type_id(&fields.agent_type_id) else {
            return;
        };
        let Some(definition) = definition_for_type(&type_id) else {
            return;
        };
        let trimmed_name = fields.name.trim();
        if trimmed_name.is_empty() {
            return;
        }
        trimmed_name.clone_into(&mut agent.name);
        agent.shortcut_slot = fields.shortcut_slot;
        agent.description.clone_from(&fields.description);
        if let Some(new_dir) = Self::validated_agent_work_dir(repository, &fields.work_dir) {
            if !repository.remote.enabled
                && !crate::services::local_paths_equivalent(
                    std::path::Path::new(&new_dir),
                    &agent.work_dir,
                )
                && let Err(error) = std::fs::create_dir_all(&new_dir)
            {
                warn!(work_dir = %new_dir, error = %error, "could not create updated local agent work directory");
            }
            agent.work_dir = std::path::PathBuf::from(new_dir);
        }
        let mut values = if agent.type_id == type_id {
            agent.values.clone()
        } else if repository.default_type_id == type_id {
            repository.default_values.clone()
        } else {
            default_values(&definition)
        };
        merge_agent_values(&definition, fields, &mut values);
        agent.type_id = type_id;
        agent.values = values;
    }
}

fn active_type_id(value: &str) -> Option<AgentTypeId> {
    super::form_projection::type_id_from_form_value(value)
}

fn definition_for_type(type_id: &AgentTypeId) -> Option<AgentDefinition> {
    super::form_projection::definition_for_type(type_id)
}

fn default_values(definition: &AgentDefinition) -> TypedMap {
    definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
        .filter_map(|field| field.default.as_ref().map(|value| (&field.id, value)))
        .filter_map(|(id, value)| Some((typed_key(id)?, typed_value(value)?)))
        .collect()
}

fn repository_values_from_fields(
    definition: &AgentDefinition,
    fields: &RepositoryFormFields,
) -> TypedMap {
    let mut values = default_values(definition);
    for field in &definition.repository_fields {
        let value = match field.id.as_str() {
            "profile" => Some(FieldValue::String(fields.default_profile.trim().to_owned())),
            "model" => Some(FieldValue::String(
                fields.default_code_puppy_model.trim().to_owned(),
            )),
            "yolo" if field.kind == FieldKind::Boolean => Some(FieldValue::Boolean(
                fields
                    .default_llxprt_mode
                    .split_whitespace()
                    .any(|value| value == "--yolo"),
            )),
            "yolo" if field.kind == FieldKind::OptionalBoolean => Some(
                FieldValue::OptionalBoolean(Some(fields.default_code_puppy_yolo)),
            ),
            _ => None,
        };
        if let Some(value) = value {
            insert_field_value(&mut values, &field.id, value);
        }
    }
    if definition
        .agent_fields
        .iter()
        .any(|field| field.id == "version_selector")
    {
        let selector = if definition
            .repository_fields
            .iter()
            .any(|field| field.id == "model")
        {
            fields.default_code_puppy_version.trim()
        } else {
            fields.default_llxprt_version.trim()
        };
        insert_field_value(
            &mut values,
            "version_selector",
            FieldValue::String(selector.to_owned()),
        );
    }
    values
}

fn merge_agent_values(
    definition: &AgentDefinition,
    fields: &AgentFormFields,
    values: &mut TypedMap,
) {
    for field in &definition.repository_fields {
        let value = match field.id.as_str() {
            "profile" => Some(FieldValue::String(fields.profile.trim().to_owned())),
            "model" => Some(FieldValue::String(
                fields.code_puppy_model.trim().to_owned(),
            )),
            "yolo" if field.kind == FieldKind::Boolean => Some(FieldValue::Boolean(
                fields
                    .mode
                    .split_whitespace()
                    .any(|value| value == "--yolo"),
            )),
            "yolo" if field.kind == FieldKind::OptionalBoolean => {
                Some(FieldValue::OptionalBoolean(Some(fields.code_puppy_yolo)))
            }
            _ => None,
        };
        if let Some(value) = value {
            insert_field_value(values, &field.id, value);
        }
    }
    for field in &definition.agent_fields {
        let value = match field.id.as_str() {
            "version_selector" => {
                let source = if definition
                    .repository_fields
                    .iter()
                    .any(|field| field.id == "model")
                {
                    &fields.code_puppy_version
                } else {
                    &fields.llxprt_version
                };
                Some(FieldValue::String(source.trim().to_owned()))
            }
            "interactive" => Some(FieldValue::Boolean(
                fields.code_puppy_quick_resume.enabled(),
            )),
            "continue" => Some(FieldValue::Boolean(fields.pass_continue)),
            _ => None,
        };
        if let Some(value) = value {
            insert_field_value(values, &field.id, value);
        }
    }
}

fn insert_field_value(values: &mut TypedMap, field: &str, value: FieldValue) {
    if let (Some(key), Some(value)) = (typed_key(field), typed_value(&value)) {
        values.insert(key, value);
    }
}

fn typed_key(field: &str) -> Option<Id> {
    Id::parse(&field.replace('_', "-")).ok()
}

fn typed_value(value: &FieldValue) -> Option<TypedValue> {
    match value {
        FieldValue::Boolean(value) => Some(TypedValue::Bool(*value)),
        FieldValue::OptionalBoolean(value) => value.map(TypedValue::Bool),
        FieldValue::String(value) | FieldValue::Path(value) => {
            Some(TypedValue::String(value.clone()))
        }
        FieldValue::Integer(value) => Some(TypedValue::Integer(*value)),
        FieldValue::StringList(values) => Some(TypedValue::List(
            values.iter().cloned().map(TypedValue::String).collect(),
        )),
    }
}

fn parse_transient_agent_dir(value: &str) -> std::path::PathBuf {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return std::path::PathBuf::new();
    }
    let expanded = expand_tilde(trimmed);
    if std::path::Path::new(&expanded)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return std::path::PathBuf::new();
    }
    std::path::PathBuf::from(expanded)
}

fn parse_transient_max_concurrent(value: &str) -> u32 {
    value.trim().parse::<u32>().unwrap_or(0)
}
