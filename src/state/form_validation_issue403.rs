//! Issue #403 form validation helpers: duplicate agent name prevention,
//! work-dir collision detection, and version whitespace validation.
//!
//! Extracted from `form_ops.rs` to keep that module under the source-size
//! limit. These are associated-function validators called by the submit
//! paths in `form_ops.rs`.

use crate::domain::{AgentId, Repository, RepositoryId};
use crate::services::local_paths_equivalent;
use crate::state::AppState;
use crate::state::types::AgentFormFields;

impl AppState {
    /// Pre-submit validation for agent form fields that should prevent
    /// creation/update and keep the modal open with a visible error.
    ///
    /// Currently checks that version fields do not contain internal
    /// whitespace (issue #403 Bug 2). The normalization layer also strips
    /// it, but surfacing the error inline before submit gives the user
    /// immediate feedback rather than silently altering their input.
    pub(super) fn validate_agent_form_fields(fields: &AgentFormFields) -> Result<(), String> {
        if has_internal_whitespace(&fields.llxprt_version) {
            return Err("LLxprt version must not contain whitespace or newlines".to_owned());
        }
        if has_internal_whitespace(&fields.code_puppy_version) {
            return Err("Code Puppy version must not contain whitespace or newlines".to_owned());
        }
        Ok(())
    }

    /// Check that no existing agent in the same repository has the same name
    /// (case-insensitive, trimmed) or a colliding work directory (issue #403
    /// Bug 1).
    pub(super) fn validate_new_agent_uniqueness(
        &self,
        repository_id: &RepositoryId,
        fields: &AgentFormFields,
        repository: &Repository,
    ) -> Result<(), String> {
        let new_name = fields.name.trim().to_lowercase();
        let new_work_dir = Self::validated_agent_work_dir(repository, &fields.work_dir);
        for agent in &self.agents {
            if &agent.repository_id != repository_id {
                continue;
            }
            if !new_name.is_empty() && agent.name.trim().to_lowercase() == new_name {
                return Err(format!(
                    "An agent named '{}' already exists in this repository",
                    fields.name.trim()
                ));
            }
            if let Some(ref new_dir) = new_work_dir
                && local_paths_equivalent(std::path::Path::new(new_dir), &agent.work_dir)
            {
                return Err(format!(
                    "Work directory '{}' is already used by agent '{}'",
                    new_dir, agent.name
                ));
            }
        }
        Ok(())
    }

    /// Same as [`validate_new_agent_uniqueness`] but excludes the agent
    /// being edited.
    pub(super) fn validate_edit_agent_uniqueness(
        &self,
        id: &AgentId,
        fields: &AgentFormFields,
        repository: &Repository,
    ) -> Result<(), String> {
        let new_name = fields.name.trim().to_lowercase();
        let new_work_dir = Self::validated_agent_work_dir(repository, &fields.work_dir);
        for agent in &self.agents {
            if &agent.id == id {
                continue;
            }
            if agent.repository_id != repository.id {
                continue;
            }
            if !new_name.is_empty() && agent.name.trim().to_lowercase() == new_name {
                return Err(format!(
                    "An agent named '{}' already exists in this repository",
                    fields.name.trim()
                ));
            }
            if let Some(ref new_dir) = new_work_dir
                && local_paths_equivalent(std::path::Path::new(new_dir), &agent.work_dir)
            {
                return Err(format!(
                    "Work directory '{}' is already used by agent '{}'",
                    new_dir, agent.name
                ));
            }
        }
        Ok(())
    }
}

/// Whether a version string contains internal whitespace after trimming
/// surrounding whitespace. Used for pre-submit validation so the user gets
/// an inline error instead of silent sanitization (issue #403).
pub(super) fn has_internal_whitespace(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().any(char::is_whitespace)
}
