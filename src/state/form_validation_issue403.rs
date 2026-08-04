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
        if fields.sandbox_enabled
            && crate::domain::SandboxEngine::from_form_value(&fields.sandbox_engine).is_none()
        {
            return Err("Sandbox engine must be Podman, Docker, or Seatbelt".to_owned());
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
        Self::check_agent_uniqueness(None, repository_id, fields, repository, &self.agents)
    }

    /// Same as [`validate_new_agent_uniqueness`] but excludes the agent
    /// being edited.
    pub(super) fn validate_edit_agent_uniqueness(
        &self,
        id: &AgentId,
        fields: &AgentFormFields,
        repository: &Repository,
    ) -> Result<(), String> {
        Self::check_agent_uniqueness(Some(id), &repository.id, fields, repository, &self.agents)
    }

    /// Shared collision-check logic for new and edit agent validation.
    ///
    /// When `exclude_id` is `Some`, that agent is skipped (edit mode). The
    /// `repository_id` is the target repository for the agent being
    /// created/edited.
    fn check_agent_uniqueness(
        exclude_id: Option<&AgentId>,
        repository_id: &RepositoryId,
        fields: &AgentFormFields,
        repository: &Repository,
        agents: &[crate::domain::Agent],
    ) -> Result<(), String> {
        let new_name = fields.name.trim().to_lowercase();
        let new_name_display = fields.name.trim().to_owned();
        let new_work_dir = Self::validated_agent_work_dir(repository, &fields.work_dir);
        for agent in agents {
            if Some(&agent.id) == exclude_id {
                continue;
            }
            if &agent.repository_id != repository_id {
                continue;
            }
            if !new_name.is_empty() && agent.name.trim().to_lowercase() == new_name {
                return Err(format!(
                    "An agent named '{new_name_display}' already exists in this repository"
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
///
/// Returns `true` when the trimmed value still contains whitespace
/// characters — i.e., there is embedded whitespace between non-whitespace
/// content. Surrounding-only whitespace (which `normalize`/`strip_internal_whitespace`
/// will remove) does **not** trigger this check, so `" nightly "` is accepted.
/// A whitespace-only string (`"   "`) trims to empty and is left to required-
/// field validation.
pub(super) fn has_internal_whitespace(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().any(char::is_whitespace)
}
