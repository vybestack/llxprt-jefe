//! Form submission reducers extracted from the near-limit form input module.

use crate::domain::RepositoryId;

use super::AppState;
use super::types::{AgentFormFields, ModalState, WorkflowDispatchFormFocus};

impl AppState {
    pub(super) fn handle_submit_form(&mut self) {
        if self.submit_generated_form() {
            return;
        }
        match self.modal.clone() {
            ModalState::NewRepository { fields, .. } => self.submit_new_repository(&fields),
            ModalState::EditRepository { id, fields, .. } => {
                self.submit_edit_repository(&id, &fields);
            }
            ModalState::NewAgent {
                repository_id,
                fields,
                ..
            } => self.submit_new_agent(&repository_id, &fields),
            ModalState::EditAgent { id, fields, .. } => self.submit_edit_agent(&id, &fields),
            ModalState::WorkflowDispatch { focus, .. } => self.submit_workflow_dispatch(focus),
            _ => self.modal = ModalState::None,
        }
    }

    fn submit_new_repository(&mut self, fields: &super::types::RepositoryFormFields) {
        if let Err(error) = crate::domain::GitHubRepoRef::parse(&fields.github_issue_pr_repo) {
            self.error_message = Some(error.to_string());
            return;
        }
        self.error_message = None;
        if let Some(repository) = Self::create_repository_from_fields(fields) {
            self.sticky_empty_repository_ids
                .insert(repository.id.clone());
            self.repositories.push(repository);
            self.selected_repository_index = Some(self.repositories.len() - 1);
            self.modal = ModalState::None;
        }
    }

    fn submit_edit_repository(
        &mut self,
        id: &RepositoryId,
        fields: &super::types::RepositoryFormFields,
    ) {
        if let Err(error) = crate::domain::GitHubRepoRef::parse(&fields.github_issue_pr_repo) {
            self.error_message = Some(error.to_string());
            return;
        }
        self.error_message = None;
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| repository.id == *id)
        else {
            return;
        };
        if Self::update_repository_from_fields(repository, fields) {
            self.modal = ModalState::None;
        }
    }

    fn submit_new_agent(&mut self, repository_id: &RepositoryId, fields: &AgentFormFields) {
        if let Err(message) = Self::validate_agent_form_fields(fields) {
            self.error_message = Some(message);
            return;
        }
        let next_display_index = self.agents.len() + 1;
        if let Some(repository) = self.repository_by_id(repository_id).cloned() {
            if let Err(message) =
                self.validate_new_agent_uniqueness(repository_id, fields, &repository)
            {
                self.error_message = Some(message);
                return;
            }
            if let Some(agent) =
                Self::create_agent_from_fields(&repository, fields, next_display_index)
            {
                self.error_message = None;
                self.enforce_shortcut_uniqueness(&agent.id, agent.shortcut_slot);
                self.agents.push(agent);
                self.selected_agent_index = Some(self.agents.len() - 1);
                self.remember_selected_agent_for_current_repo();
                self.modal = ModalState::None;
            }
        }
    }

    fn submit_edit_agent(&mut self, id: &crate::domain::AgentId, fields: &AgentFormFields) {
        if fields.name.trim().is_empty() {
            return;
        }
        if let Err(message) = Self::validate_agent_form_fields(fields) {
            self.error_message = Some(message);
            return;
        }

        self.enforce_shortcut_uniqueness(id, fields.shortcut_slot);
        let repository = self.repository_for_agent(id).cloned();
        if let Some(repository) = repository {
            if Self::validated_agent_work_dir(&repository, &fields.work_dir).is_none() {
                return;
            }
            if let Err(message) = self.validate_edit_agent_uniqueness(id, fields, &repository) {
                self.error_message = Some(message);
                return;
            }
            if let Some(agent) = self.agents.iter_mut().find(|agent| &agent.id == id) {
                self.error_message = None;
                Self::update_agent_from_fields(agent, &repository, fields);
            }
        }
        self.remember_selected_agent_for_current_repo();
        self.modal = ModalState::None;
    }

    fn submit_workflow_dispatch(&mut self, focus: WorkflowDispatchFormFocus) {
        if focus == WorkflowDispatchFormFocus::Cancel {
            self.modal = ModalState::None;
        }
    }

    /// Parse newline-separated workflow dispatch `key=value` pairs.
    #[must_use]
    pub fn parse_workflow_dispatch_inputs(inputs: &str) -> Vec<(String, String)> {
        crate::state::form_workflow_dispatch::parse_inputs(inputs)
    }
}
