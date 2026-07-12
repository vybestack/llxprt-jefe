//! Form input handling: character insertion, deletion, cursor movement, field
//! navigation, checkbox toggling, and form submission logic.

use crate::domain::{AgentKind, RepositoryId, SandboxEngine};

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus, WorkflowDispatchFormFocus,
};
use super::util::{delete_char_at, delete_char_before, insert_char_at};

impl AppState {
    fn adjacent_repository_focus(
        fields: &RepositoryFormFields,
        focus: RepositoryFormFocus,
        step: fn(RepositoryFormFocus) -> RepositoryFormFocus,
    ) -> RepositoryFormFocus {
        let candidate = step(focus);
        if candidate == RepositoryFormFocus::DefaultCodePuppyModel
            && AgentKind::from_form_value(&fields.default_agent_kind) != Some(AgentKind::CodePuppy)
        {
            step(candidate)
        } else {
            candidate
        }
    }

    fn handle_agent_shortcut_char(fields: &mut AgentFormFields, c: char) {
        if c == '0' {
            fields.shortcut_slot = None;
        } else if let Some(digit) = c.to_digit(10)
            && (1..=9).contains(&digit)
        {
            fields.shortcut_slot = u8::try_from(digit).ok();
        }
    }

    fn handle_agent_field_char(
        installed: &[AgentKind],
        fields: &mut AgentFormFields,
        cursor: &mut AgentFormCursor,
        focus: AgentFormFocus,
        c: char,
    ) -> bool {
        match focus {
            AgentFormFocus::Shortcut => {
                Self::handle_agent_shortcut_char(fields, c);
                false
            }
            AgentFormFocus::Name => {
                cursor.name = insert_char_at(&mut fields.name, cursor.name, c);
                true
            }
            AgentFormFocus::Description => {
                cursor.description = insert_char_at(&mut fields.description, cursor.description, c);
                false
            }
            AgentFormFocus::WorkDir => {
                cursor.work_dir = insert_char_at(&mut fields.work_dir, cursor.work_dir, c);
                false
            }
            AgentFormFocus::Profile => {
                cursor.profile = insert_char_at(&mut fields.profile, cursor.profile, c);
                false
            }
            AgentFormFocus::CodePuppyModel => {
                cursor.code_puppy_model =
                    insert_char_at(&mut fields.code_puppy_model, cursor.code_puppy_model, c);
                false
            }
            AgentFormFocus::Mode => {
                cursor.mode = insert_char_at(&mut fields.mode, cursor.mode, c);
                false
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    insert_char_at(&mut fields.llxprt_debug, cursor.llxprt_debug, c);
                false
            }
            AgentFormFocus::AgentKind
            | AgentFormFocus::CodePuppyYolo
            | AgentFormFocus::CodePuppyQuickResume
            | AgentFormFocus::PassContinue
            | AgentFormFocus::Sandbox
            | AgentFormFocus::SandboxEngine => {
                super::form_runtime::cycle_agent_field(installed, fields, focus, c);
                false
            }
            AgentFormFocus::SandboxFlags => {
                cursor.sandbox_flags =
                    insert_char_at(&mut fields.sandbox_flags, cursor.sandbox_flags, c);
                false
            }
        }
    }

    fn handle_new_agent_char(
        installed: &[AgentKind],
        fields: &mut AgentFormFields,
        cursor: &mut AgentFormCursor,
        focus: AgentFormFocus,
        work_dir_manual: &mut bool,
        c: char,
    ) -> bool {
        if focus == AgentFormFocus::WorkDir {
            *work_dir_manual = true;
        }
        Self::handle_agent_field_char(installed, fields, cursor, focus, c) && !*work_dir_manual
    }

    fn effective_agent_kinds_for_current_form(&self) -> Vec<AgentKind> {
        let is_remote = match &self.modal {
            ModalState::NewAgent { repository_id, .. } => self
                .repository_by_id(repository_id)
                .is_some_and(|repo| repo.remote.enabled),
            ModalState::EditAgent { id, .. } => self
                .repository_for_agent(id)
                .is_some_and(|repo| repo.remote.enabled),
            _ => false,
        };
        super::form_runtime::effective_agent_kinds(&self.installed_agent_kinds, is_remote)
    }

    /// Resolve effective agent kinds for a repository form (New/Edit).
    ///
    /// Repository forms with `remote_enabled` offer both AgentKind variants
    /// regardless of local installed snapshot. Local forms offer installed
    /// kinds only. This matches what the UI hint and the selection projection
    /// render.
    fn effective_agent_kinds_for_repository_form(&self) -> Vec<AgentKind> {
        let is_remote = match &self.modal {
            ModalState::NewRepository { fields, .. }
            | ModalState::EditRepository { fields, .. } => fields.remote_enabled,
            _ => false,
        };
        super::form_runtime::effective_agent_kinds(&self.installed_agent_kinds, is_remote)
    }

    pub(super) fn handle_form_char(&mut self, c: char) {
        let agent_kinds = self.effective_agent_kinds_for_current_form();
        let repo_kinds = self.effective_agent_kinds_for_repository_form();
        let refresh_work_dir = self.form_char_refreshes_work_dir(&agent_kinds, &repo_kinds, c);

        if refresh_work_dir {
            self.refresh_new_agent_work_dir();
        }
    }

    /// Dispatch a typed character to the focused form field and return whether
    /// the new-agent work-dir should be refreshed afterwards.
    fn form_char_refreshes_work_dir(
        &mut self,
        agent_kinds: &[AgentKind],
        repo_kinds: &[AgentKind],
        c: char,
    ) -> bool {
        match &mut self.modal {
            ModalState::Search { query } => {
                query.push(c);
                false
            }
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => {
                if crate::state::form_cursor::handle_repository_field_char(
                    fields, cursor, *focus, c,
                ) {
                    Self::toggle_repository_checkbox(repo_kinds, fields, *focus);
                }
                false
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                work_dir_manual,
                ..
            } => {
                Self::handle_new_agent_char(agent_kinds, fields, cursor, *focus, work_dir_manual, c)
            }
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                let _ = Self::handle_agent_field_char(agent_kinds, fields, cursor, *focus, c);
                false
            }
            ModalState::WorkflowDispatch {
                fields,
                focus,
                cursor,
                ..
            } => {
                crate::state::form_workflow_dispatch::handle_field_char(fields, cursor, *focus, c);
                false
            }
            _ => false,
        }
    }

    fn refresh_new_agent_work_dir(&mut self) {
        self.update_agent_work_dir_from_name();
        if let ModalState::NewAgent { fields, cursor, .. } = &mut self.modal {
            cursor.work_dir = fields.work_dir.chars().count();
        }
    }

    pub(super) fn delete_repository_field_before_cursor(
        fields: &mut RepositoryFormFields,
        cursor: &mut RepositoryFormCursor,
        focus: RepositoryFormFocus,
    ) {
        match focus {
            RepositoryFormFocus::Name => {
                cursor.name = delete_char_before(&mut fields.name, cursor.name);
            }
            RepositoryFormFocus::BaseDir => {
                cursor.base_dir = delete_char_before(&mut fields.base_dir, cursor.base_dir);
            }
            RepositoryFormFocus::DefaultProfile => {
                cursor.default_profile =
                    delete_char_before(&mut fields.default_profile, cursor.default_profile);
            }
            RepositoryFormFocus::DefaultCodePuppyModel => {
                cursor.default_code_puppy_model = delete_char_before(
                    &mut fields.default_code_puppy_model,
                    cursor.default_code_puppy_model,
                );
            }
            RepositoryFormFocus::GitHubRepo => {
                cursor.github_repo =
                    delete_char_before(&mut fields.github_repo, cursor.github_repo);
            }
            RepositoryFormFocus::LoginUser => {
                cursor.login_user = delete_char_before(&mut fields.login_user, cursor.login_user);
            }
            RepositoryFormFocus::Host => {
                cursor.host = delete_char_before(&mut fields.host, cursor.host);
            }
            RepositoryFormFocus::RunAsUser => {
                cursor.run_as_user =
                    delete_char_before(&mut fields.run_as_user, cursor.run_as_user);
            }
            RepositoryFormFocus::DefaultAgentKind
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::SetupEnvDefault => {}
        }
    }

    pub(super) fn delete_repository_field_at_cursor(
        fields: &mut RepositoryFormFields,
        cursor: &RepositoryFormCursor,
        focus: RepositoryFormFocus,
    ) {
        match focus {
            RepositoryFormFocus::Name => {
                delete_char_at(&mut fields.name, cursor.name);
            }
            RepositoryFormFocus::BaseDir => {
                delete_char_at(&mut fields.base_dir, cursor.base_dir);
            }
            RepositoryFormFocus::DefaultProfile => {
                delete_char_at(&mut fields.default_profile, cursor.default_profile);
            }
            RepositoryFormFocus::DefaultCodePuppyModel => {
                delete_char_at(
                    &mut fields.default_code_puppy_model,
                    cursor.default_code_puppy_model,
                );
            }
            RepositoryFormFocus::GitHubRepo => {
                delete_char_at(&mut fields.github_repo, cursor.github_repo);
            }
            RepositoryFormFocus::LoginUser => {
                delete_char_at(&mut fields.login_user, cursor.login_user);
            }
            RepositoryFormFocus::Host => {
                delete_char_at(&mut fields.host, cursor.host);
            }
            RepositoryFormFocus::RunAsUser => {
                delete_char_at(&mut fields.run_as_user, cursor.run_as_user);
            }
            RepositoryFormFocus::DefaultAgentKind
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::SetupEnvDefault => {}
        }
    }

    pub(super) fn delete_agent_field_before_cursor(
        fields: &mut AgentFormFields,
        cursor: &mut AgentFormCursor,
        focus: AgentFormFocus,
    ) {
        match focus {
            AgentFormFocus::Shortcut => {
                fields.shortcut_slot = None;
            }
            AgentFormFocus::Name => {
                cursor.name = delete_char_before(&mut fields.name, cursor.name);
            }
            AgentFormFocus::Description => {
                cursor.description =
                    delete_char_before(&mut fields.description, cursor.description);
            }
            AgentFormFocus::WorkDir => {
                cursor.work_dir = delete_char_before(&mut fields.work_dir, cursor.work_dir);
            }
            AgentFormFocus::Profile => {
                cursor.profile = delete_char_before(&mut fields.profile, cursor.profile);
            }
            AgentFormFocus::CodePuppyModel => {
                cursor.code_puppy_model =
                    delete_char_before(&mut fields.code_puppy_model, cursor.code_puppy_model);
            }
            AgentFormFocus::Mode => {
                cursor.mode = delete_char_before(&mut fields.mode, cursor.mode);
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    delete_char_before(&mut fields.llxprt_debug, cursor.llxprt_debug);
            }
            AgentFormFocus::AgentKind
            | AgentFormFocus::CodePuppyYolo
            | AgentFormFocus::CodePuppyQuickResume
            | AgentFormFocus::PassContinue
            | AgentFormFocus::Sandbox
            | AgentFormFocus::SandboxEngine => {}
            AgentFormFocus::SandboxFlags => {
                cursor.sandbox_flags =
                    delete_char_before(&mut fields.sandbox_flags, cursor.sandbox_flags);
            }
        }
    }

    pub(super) fn delete_agent_field_at_cursor(
        fields: &mut AgentFormFields,
        cursor: &AgentFormCursor,
        focus: AgentFormFocus,
    ) {
        match focus {
            AgentFormFocus::Shortcut
            | AgentFormFocus::AgentKind
            | AgentFormFocus::CodePuppyYolo
            | AgentFormFocus::CodePuppyQuickResume
            | AgentFormFocus::PassContinue
            | AgentFormFocus::Sandbox
            | AgentFormFocus::SandboxEngine => {}
            AgentFormFocus::Name => {
                delete_char_at(&mut fields.name, cursor.name);
            }
            AgentFormFocus::Description => {
                delete_char_at(&mut fields.description, cursor.description);
            }
            AgentFormFocus::WorkDir => {
                delete_char_at(&mut fields.work_dir, cursor.work_dir);
            }
            AgentFormFocus::Profile => {
                delete_char_at(&mut fields.profile, cursor.profile);
            }
            AgentFormFocus::CodePuppyModel => {
                delete_char_at(&mut fields.code_puppy_model, cursor.code_puppy_model);
            }
            AgentFormFocus::Mode => {
                delete_char_at(&mut fields.mode, cursor.mode);
            }
            AgentFormFocus::LlxprtDebug => {
                delete_char_at(&mut fields.llxprt_debug, cursor.llxprt_debug);
            }
            AgentFormFocus::SandboxFlags => {
                delete_char_at(&mut fields.sandbox_flags, cursor.sandbox_flags);
            }
        }
    }

    pub(super) fn handle_form_backspace(&mut self) {
        let mut refresh_work_dir = false;

        match &mut self.modal {
            ModalState::Search { query } => {
                query.pop();
            }
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => {
                Self::delete_repository_field_before_cursor(fields, cursor, *focus);
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                work_dir_manual,
                ..
            } => {
                let focused = *focus;
                Self::delete_agent_field_before_cursor(fields, cursor, focused);
                if focused == AgentFormFocus::WorkDir {
                    *work_dir_manual = true;
                } else if focused == AgentFormFocus::Name && !*work_dir_manual {
                    refresh_work_dir = true;
                }
            }
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                Self::delete_agent_field_before_cursor(fields, cursor, *focus);
            }
            ModalState::WorkflowDispatch {
                fields,
                focus,
                cursor,
                ..
            } => {
                crate::state::form_workflow_dispatch::delete_field_before_cursor(
                    fields, cursor, *focus,
                );
            }
            _ => {}
        }

        if refresh_work_dir {
            self.update_agent_work_dir_from_name();
            if let ModalState::NewAgent { fields, cursor, .. } = &mut self.modal {
                cursor.work_dir = fields.work_dir.chars().count();
            }
        }
    }

    pub(super) fn handle_form_delete(&mut self) {
        let mut refresh_work_dir = false;

        match &mut self.modal {
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => {
                Self::delete_repository_field_at_cursor(fields, cursor, *focus);
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                work_dir_manual,
                ..
            } => {
                let focused = *focus;
                Self::delete_agent_field_at_cursor(fields, cursor, focused);
                if focused == AgentFormFocus::WorkDir {
                    *work_dir_manual = true;
                } else if focused == AgentFormFocus::Name && !*work_dir_manual {
                    refresh_work_dir = true;
                }
            }
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                Self::delete_agent_field_at_cursor(fields, cursor, *focus);
            }
            ModalState::WorkflowDispatch {
                fields,
                focus,
                cursor,
                ..
            } => {
                crate::state::form_workflow_dispatch::delete_field_at_cursor(
                    fields, cursor, *focus,
                );
            }
            _ => {}
        }

        if refresh_work_dir {
            self.update_agent_work_dir_from_name();
            if let ModalState::NewAgent { fields, cursor, .. } = &mut self.modal {
                cursor.work_dir = fields.work_dir.chars().count();
            }
        }
    }

    pub(super) fn handle_form_move_cursor_left(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, cursor, .. }
            | ModalState::EditRepository { focus, cursor, .. } => {
                crate::state::form_cursor::move_repository_field_cursor_left(cursor, *focus);
            }
            ModalState::NewAgent { focus, cursor, .. }
            | ModalState::EditAgent { focus, cursor, .. } => {
                crate::state::form_cursor::move_agent_field_cursor_left(cursor, *focus);
            }
            ModalState::WorkflowDispatch { focus, cursor, .. } => {
                crate::state::form_workflow_dispatch::move_cursor_field_left(cursor, *focus);
            }
            _ => {}
        }
    }

    pub(super) fn handle_form_move_cursor_right(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => crate::state::form_cursor::move_repository_field_cursor_right(
                fields, cursor, *focus,
            ),
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => crate::state::form_cursor::move_agent_field_cursor_right(fields, cursor, *focus),
            ModalState::WorkflowDispatch {
                fields,
                focus,
                cursor,
                ..
            } => crate::state::form_cursor::move_workflow_dispatch_field_cursor_right(
                fields, cursor, *focus,
            ),
            _ => {}
        }
    }

    pub(super) fn handle_form_next_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                *focus = Self::adjacent_repository_focus(fields, *focus, RepositoryFormFocus::next);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                let visibility = super::form_projection::agent_form_visibility(
                    super::form_projection::kind_from_form_value(&fields.agent_kind),
                );
                *focus = super::form_projection::next_visible_focus(*focus, visibility);
            }
            ModalState::WorkflowDispatch { focus, .. } => {
                *focus = focus.next();
            }
            _ => {}
        }
    }

    pub(super) fn handle_form_prev_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                *focus = Self::adjacent_repository_focus(fields, *focus, RepositoryFormFocus::prev);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                let visibility = super::form_projection::agent_form_visibility(
                    super::form_projection::kind_from_form_value(&fields.agent_kind),
                );
                *focus = super::form_projection::prev_visible_focus(*focus, visibility);
            }
            ModalState::WorkflowDispatch { focus, .. } => {
                *focus = focus.prev();
            }
            _ => {}
        }
    }

    pub(super) fn toggle_repository_checkbox(
        installed: &[AgentKind],
        fields: &mut RepositoryFormFields,
        focus: RepositoryFormFocus,
    ) {
        match focus {
            RepositoryFormFocus::DefaultAgentKind => {
                if let Some(next) =
                    super::form_runtime::next_installed_kind(installed, &fields.default_agent_kind)
                {
                    next.label().clone_into(&mut fields.default_agent_kind);
                }
            }
            RepositoryFormFocus::RemoteEnabled => fields.remote_enabled = !fields.remote_enabled,
            RepositoryFormFocus::SetupEnvDefault => {
                fields.setup_env_default = !fields.setup_env_default;
            }
            _ => {}
        }
    }

    pub(super) fn handle_form_toggle_checkbox(&mut self) {
        // Resolve effective agent kinds BEFORE the mutable modal match to
        // avoid borrowing self twice (kind resolution reads
        // repository/installed-agent state).
        let agent_kinds = self.effective_agent_kinds_for_current_form();
        let repo_kinds = self.effective_agent_kinds_for_repository_form();

        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                Self::toggle_repository_checkbox(&repo_kinds, fields, *focus);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                if matches!(focus, AgentFormFocus::AgentKind) {
                    super::form_runtime::cycle_agent_field(&agent_kinds, fields, *focus, ' ');
                }
                Self::toggle_agent_checkbox_fields(fields, *focus);
            }
            ModalState::ConfirmDeleteAgent {
                delete_work_dir, ..
            } => {
                *delete_work_dir = !*delete_work_dir;
            }
            _ => {}
        }
    }

    /// Toggle non-AgentKind checkbox fields for agent forms (PassContinue,
    /// Shortcut, Sandbox, SandboxEngine). AgentKind is handled separately
    /// because it depends on the effective kind list (remote vs local).
    fn toggle_agent_checkbox_fields(fields: &mut AgentFormFields, focus: AgentFormFocus) {
        match focus {
            AgentFormFocus::CodePuppyYolo => fields.code_puppy_yolo = !fields.code_puppy_yolo,
            AgentFormFocus::CodePuppyQuickResume => {
                fields.code_puppy_quick_resume.toggle();
            }
            AgentFormFocus::PassContinue => fields.pass_continue = !fields.pass_continue,
            AgentFormFocus::Shortcut => {
                fields.shortcut_slot = match fields.shortcut_slot {
                    None => Some(1),
                    Some(9) => None,
                    Some(slot) => Some(slot + 1),
                };
            }
            AgentFormFocus::Sandbox => fields.sandbox_enabled = !fields.sandbox_enabled,
            AgentFormFocus::SandboxEngine => {
                SandboxEngine::next_from_form_value(&fields.sandbox_engine)
                    .label()
                    .clone_into(&mut fields.sandbox_engine);
            }
            _ => {}
        }
    }

    pub(super) fn update_agent_work_dir_from_name(&mut self) {
        if let ModalState::NewAgent {
            repository_id,
            fields,
            work_dir_manual,
            ..
        } = &mut self.modal
        {
            if *work_dir_manual {
                return;
            }
            let base_dir = self
                .repositories
                .iter()
                .find(|r| r.id == *repository_id)
                .map_or_else(
                    || "/tmp".to_owned(),
                    |r| r.base_dir.to_string_lossy().into_owned(),
                );
            fields.work_dir =
                super::form_runtime::derive_work_dir_from_name(&fields.name, &base_dir);
        }
    }

    pub(super) fn handle_submit_form(&mut self) {
        match self.modal.clone() {
            ModalState::NewRepository { fields, .. } => {
                if let Some(repo) = Self::create_repository_from_fields(&fields) {
                    self.repositories.push(repo);
                    self.selected_repository_index = Some(self.repositories.len() - 1);
                    self.modal = ModalState::None;
                }
            }
            ModalState::EditRepository { id, fields, .. } => {
                let Some(repo) = self.repositories.iter_mut().find(|r| r.id == id) else {
                    return;
                };
                if Self::update_repository_from_fields(repo, &fields) {
                    self.modal = ModalState::None;
                }
            }
            ModalState::NewAgent {
                repository_id,
                fields,
                ..
            } => self.submit_new_agent(&repository_id, &fields),
            ModalState::EditAgent { id, fields, .. } => self.submit_edit_agent(&id, &fields),
            ModalState::WorkflowDispatch { focus, .. } => self.submit_workflow_dispatch(focus),
            _ => {
                self.modal = ModalState::None;
            }
        }
    }

    fn submit_new_agent(&mut self, repository_id: &RepositoryId, fields: &AgentFormFields) {
        let next_display_index = self.agents.len() + 1;
        if let Some(repository) = self.repository_by_id(repository_id).cloned()
            && let Some(agent) =
                Self::create_agent_from_fields(&repository, fields, next_display_index)
        {
            self.enforce_shortcut_uniqueness(&agent.id, agent.shortcut_slot);
            self.agents.push(agent);
            self.selected_agent_index = Some(self.agents.len() - 1);
            self.remember_selected_agent_for_current_repo();
            self.modal = ModalState::None;
        }
    }

    fn submit_edit_agent(&mut self, id: &crate::domain::AgentId, fields: &AgentFormFields) {
        if fields.name.trim().is_empty() {
            return;
        }

        self.enforce_shortcut_uniqueness(id, fields.shortcut_slot);
        let repository = self.repository_for_agent(id).cloned();
        if let Some(repository) = repository {
            if Self::validated_agent_work_dir(&repository, &fields.work_dir).is_none() {
                return;
            }
            if let Some(agent) = self.agents.iter_mut().find(|a| &a.id == id) {
                Self::update_agent_from_fields(agent, &repository, fields);
            }
        }
        self.remember_selected_agent_for_current_repo();
        self.modal = ModalState::None;
    }

    fn submit_workflow_dispatch(&mut self, focus: WorkflowDispatchFormFocus) {
        match focus {
            WorkflowDispatchFormFocus::Cancel => {
                self.modal = ModalState::None;
            }
            // The authoritative submit path is `handle_workflow_dispatch_submit`
            // in app_input/modal_handlers.rs, which validates the ref, resolves
            // the repository, closes the modal, and emits the dispatch message.
            // This reducer arm must NOT close the modal on its own: if a stray
            // `SubmitForm` ever reaches a WorkflowDispatch modal without going
            // through the handler, closing here would silently swallow the
            // dispatch. Leave the modal open so the user can retry.
            WorkflowDispatchFormFocus::Submit
            | WorkflowDispatchFormFocus::RefName
            | WorkflowDispatchFormFocus::Inputs => {}
        }
    }

    /// Validate a WorkflowDispatch submit. Returns the parsed params if the
    /// ref_name is non-empty, or `None` if validation failed (in which case
    /// the caller should keep the modal open). Delegates to the pure parser
    /// in [`form_workflow_dispatch`] (newline-separated `key=value` pairs).
    #[must_use]
    pub fn parse_workflow_dispatch_inputs(inputs: &str) -> Vec<(String, String)> {
        crate::state::form_workflow_dispatch::parse_inputs(inputs)
    }
}

#[cfg(test)]
#[path = "form_ops_tests.rs"]
mod tests;
