//! Form input handling: character insertion, deletion, cursor movement, field
//! navigation, checkbox toggling, and form submission logic.

use crate::domain::SandboxEngine;
use crate::domain::agent_definition::AgentTypeId;

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus,
};
use super::util::{delete_char_at, delete_char_before, insert_char_at};

impl AppState {
    fn adjacent_repository_focus(
        fields: &RepositoryFormFields,
        focus: RepositoryFormFocus,
        forward: bool,
    ) -> RepositoryFormFocus {
        let Some(type_id) = crate::state::type_id_from_form_value(&fields.default_type_id) else {
            return focus;
        };
        if forward {
            crate::state::next_visible_repository_focus(focus, &type_id)
        } else {
            crate::state::prev_visible_repository_focus(focus, &type_id)
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
        installed: &[AgentTypeId],
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
            AgentFormFocus::CodePuppyVersion => {
                cursor.code_puppy_version =
                    insert_char_at(&mut fields.code_puppy_version, cursor.code_puppy_version, c);
                false
            }
            AgentFormFocus::Mode => {
                cursor.mode = insert_char_at(&mut fields.mode, cursor.mode, c);
                false
            }
            AgentFormFocus::LlxprtVersion => {
                cursor.llxprt_version =
                    insert_char_at(&mut fields.llxprt_version, cursor.llxprt_version, c);
                false
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    insert_char_at(&mut fields.llxprt_debug, cursor.llxprt_debug, c);
                false
            }
            AgentFormFocus::AgentType
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
        installed: &[AgentTypeId],
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

    fn effective_agent_type_ids_for_current_form(&self) -> Vec<AgentTypeId> {
        let is_remote = match &self.modal {
            ModalState::NewAgent { repository_id, .. } => self
                .repository_by_id(repository_id)
                .is_some_and(|repo| repo.remote.enabled),
            ModalState::EditAgent { id, .. } => self
                .repository_for_agent(id)
                .is_some_and(|repo| repo.remote.enabled),
            _ => false,
        };
        super::form_runtime::effective_agent_type_ids(&self.available_agent_type_ids, is_remote)
    }

    /// Resolve effective agent kinds for a repository form (New/Edit).
    ///
    /// Repository forms with `remote_enabled` offer both AgentTypeId variants
    /// regardless of local installed snapshot. Local forms offer installed
    /// kinds only. This matches what the UI hint and the selection projection
    /// render.
    fn effective_agent_type_ids_for_repository_form(&self) -> Vec<AgentTypeId> {
        let is_remote = match &self.modal {
            ModalState::NewRepository { fields, .. }
            | ModalState::EditRepository { fields, .. } => fields.remote_enabled,
            _ => false,
        };
        super::form_runtime::effective_agent_type_ids(&self.available_agent_type_ids, is_remote)
    }

    pub(super) fn handle_form_char(&mut self, c: char) {
        if self.handle_generated_form_intent(
            super::generated_agent_form::GeneratedAgentFormIntent::Insert(c),
        ) {
            return;
        }
        let agent_type_ids = self.effective_agent_type_ids_for_current_form();
        let repository_type_ids = self.effective_agent_type_ids_for_repository_form();
        let refresh_work_dir =
            self.form_char_refreshes_work_dir(&agent_type_ids, &repository_type_ids, c);

        if refresh_work_dir {
            self.refresh_new_agent_work_dir();
        }
    }

    /// Dispatch a typed character to the focused form field and return whether
    /// the new-agent work-dir should be refreshed afterwards.
    fn form_char_refreshes_work_dir(
        &mut self,
        agent_type_ids: &[AgentTypeId],
        repository_type_ids: &[AgentTypeId],
        c: char,
    ) -> bool {
        if self.active_overlay_kind() == Some(crate::workbench::OverlayKind::Search) {
            self.push_search_char(c);
            return false;
        }
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
                if crate::state::form_cursor::handle_repository_field_char(
                    fields, cursor, *focus, c,
                ) {
                    Self::toggle_repository_checkbox(repository_type_ids, fields, *focus);
                }
                false
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                work_dir_manual,
                ..
            } => Self::handle_new_agent_char(
                agent_type_ids,
                fields,
                cursor,
                *focus,
                work_dir_manual,
                c,
            ),
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                let _ = Self::handle_agent_field_char(agent_type_ids, fields, cursor, *focus, c);
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
            RepositoryFormFocus::DefaultCodePuppyVersion => {
                cursor.default_code_puppy_version = delete_char_before(
                    &mut fields.default_code_puppy_version,
                    cursor.default_code_puppy_version,
                );
            }
            RepositoryFormFocus::DefaultLlxprtMode => {
                cursor.default_llxprt_mode =
                    delete_char_before(&mut fields.default_llxprt_mode, cursor.default_llxprt_mode);
            }
            RepositoryFormFocus::DefaultLlxprtVersion => {
                cursor.default_llxprt_version = delete_char_before(
                    &mut fields.default_llxprt_version,
                    cursor.default_llxprt_version,
                );
            }
            RepositoryFormFocus::TransientAgentDir
            | RepositoryFormFocus::TransientMaxConcurrent => {
                super::form_delete_helpers::delete_transient_field_before(fields, cursor, focus);
            }
            RepositoryFormFocus::GitHubRepo => {
                cursor.github_repo =
                    delete_char_before(&mut fields.github_repo, cursor.github_repo);
            }
            RepositoryFormFocus::IssuePrRepo => {
                cursor.github_issue_pr_repo = delete_char_before(
                    &mut fields.github_issue_pr_repo,
                    cursor.github_issue_pr_repo,
                );
            }
            RepositoryFormFocus::LoginUser
            | RepositoryFormFocus::Host
            | RepositoryFormFocus::SshPort
            | RepositoryFormFocus::IdentityFile
            | RepositoryFormFocus::SshOptions
            | RepositoryFormFocus::RunAsUser => {
                super::form_delete_helpers::delete_remote_field_before(fields, cursor, focus);
            }
            RepositoryFormFocus::DefaultAgentType
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::DefaultCodePuppyYolo
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
            RepositoryFormFocus::DefaultCodePuppyVersion => {
                delete_char_at(
                    &mut fields.default_code_puppy_version,
                    cursor.default_code_puppy_version,
                );
            }
            RepositoryFormFocus::DefaultLlxprtMode => {
                delete_char_at(&mut fields.default_llxprt_mode, cursor.default_llxprt_mode);
            }
            RepositoryFormFocus::DefaultLlxprtVersion => {
                delete_char_at(
                    &mut fields.default_llxprt_version,
                    cursor.default_llxprt_version,
                );
            }
            RepositoryFormFocus::TransientAgentDir
            | RepositoryFormFocus::TransientMaxConcurrent => {
                super::form_delete_helpers::delete_transient_field_at(fields, cursor, focus);
            }
            RepositoryFormFocus::GitHubRepo => {
                delete_char_at(&mut fields.github_repo, cursor.github_repo);
            }
            RepositoryFormFocus::IssuePrRepo => delete_char_at(
                &mut fields.github_issue_pr_repo,
                cursor.github_issue_pr_repo,
            ),
            RepositoryFormFocus::LoginUser
            | RepositoryFormFocus::Host
            | RepositoryFormFocus::SshPort
            | RepositoryFormFocus::IdentityFile
            | RepositoryFormFocus::SshOptions
            | RepositoryFormFocus::RunAsUser => {
                super::form_delete_helpers::delete_remote_field_at(fields, cursor, focus);
            }
            RepositoryFormFocus::DefaultAgentType
            | RepositoryFormFocus::RemoteEnabled
            | RepositoryFormFocus::DefaultCodePuppyYolo
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
            AgentFormFocus::CodePuppyVersion => {
                cursor.code_puppy_version =
                    delete_char_before(&mut fields.code_puppy_version, cursor.code_puppy_version);
            }
            AgentFormFocus::Mode => {
                cursor.mode = delete_char_before(&mut fields.mode, cursor.mode);
            }
            AgentFormFocus::LlxprtVersion => {
                cursor.llxprt_version =
                    delete_char_before(&mut fields.llxprt_version, cursor.llxprt_version);
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    delete_char_before(&mut fields.llxprt_debug, cursor.llxprt_debug);
            }
            AgentFormFocus::AgentType
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
            | AgentFormFocus::AgentType
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
            AgentFormFocus::CodePuppyVersion => {
                delete_char_at(&mut fields.code_puppy_version, cursor.code_puppy_version);
            }
            AgentFormFocus::Mode => {
                delete_char_at(&mut fields.mode, cursor.mode);
            }
            AgentFormFocus::LlxprtVersion => {
                delete_char_at(&mut fields.llxprt_version, cursor.llxprt_version);
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
        if self.active_overlay_kind() == Some(crate::workbench::OverlayKind::Search) {
            self.pop_search_char();
            return;
        }
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
                Self::delete_repository_field_before_cursor(fields, cursor, *focus);
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                let f = *focus;
                Self::delete_agent_field_before_cursor(fields, cursor, f);
                self.after_agent_delete(f);
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
    }

    pub(super) fn handle_form_delete(&mut self) {
        if self.handle_generated_form_intent(
            super::generated_agent_form::GeneratedAgentFormIntent::Delete,
        ) {
            return;
        }
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
                ..
            } => {
                let f = *focus;
                Self::delete_agent_field_at_cursor(fields, cursor, f);
                self.after_agent_delete(f);
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
    }

    fn after_agent_delete(&mut self, focused: AgentFormFocus) {
        let need_refresh = matches!(&self.modal, ModalState::NewAgent { work_dir_manual, .. } if !*work_dir_manual)
            && focused == AgentFormFocus::Name;
        if let ModalState::NewAgent {
            work_dir_manual, ..
        } = &mut self.modal
            && focused == AgentFormFocus::WorkDir
        {
            *work_dir_manual = true;
        }
        if need_refresh {
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
        if self.handle_generated_form_intent(
            super::generated_agent_form::GeneratedAgentFormIntent::Next,
        ) {
            return;
        }
        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                *focus = Self::adjacent_repository_focus(fields, *focus, true);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                let type_id =
                    super::form_projection::type_id_from_form_value(&fields.agent_type_id);
                let visibility = super::form_projection::agent_form_visibility(type_id.as_ref());
                *focus = super::form_projection::next_visible_focus(*focus, &visibility);
            }
            ModalState::WorkflowDispatch { focus, .. } => {
                *focus = focus.next();
            }
            _ => {}
        }
    }

    pub(super) fn handle_form_prev_field(&mut self) {
        if self.handle_generated_form_intent(
            super::generated_agent_form::GeneratedAgentFormIntent::Previous,
        ) {
            return;
        }
        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                *focus = Self::adjacent_repository_focus(fields, *focus, false);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                let type_id =
                    super::form_projection::type_id_from_form_value(&fields.agent_type_id);
                let visibility = super::form_projection::agent_form_visibility(type_id.as_ref());
                *focus = super::form_projection::prev_visible_focus(*focus, &visibility);
            }
            ModalState::WorkflowDispatch { focus, .. } => {
                *focus = focus.prev();
            }
            _ => {}
        }
    }

    pub(super) fn toggle_repository_checkbox(
        installed: &[AgentTypeId],
        fields: &mut RepositoryFormFields,
        focus: RepositoryFormFocus,
    ) {
        match focus {
            RepositoryFormFocus::DefaultAgentType => {
                if let Some(next) =
                    super::form_runtime::next_available_type(installed, &fields.default_type_id)
                {
                    next.as_str().clone_into(&mut fields.default_type_id);
                }
            }
            RepositoryFormFocus::RemoteEnabled => fields.remote_enabled = !fields.remote_enabled,
            RepositoryFormFocus::DefaultCodePuppyYolo => {
                fields.default_code_puppy_yolo = !fields.default_code_puppy_yolo;
            }
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
        let agent_type_ids = self.effective_agent_type_ids_for_current_form();
        let repository_type_ids = self.effective_agent_type_ids_for_repository_form();

        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                Self::toggle_repository_checkbox(&repository_type_ids, fields, *focus);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => {
                if matches!(focus, AgentFormFocus::AgentType) {
                    super::form_runtime::cycle_agent_field(&agent_type_ids, fields, *focus, ' ');
                }
                Self::toggle_agent_checkbox_fields(fields, *focus);
            }
            _ => {}
        }
    }

    /// Toggle non-AgentTypeId checkbox fields for agent forms (PassContinue,
    /// Shortcut, Sandbox, SandboxEngine). AgentTypeId is handled separately
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
            fields.work_dir = self
                .repositories
                .iter()
                .find(|r| r.id == *repository_id)
                .map_or_else(
                    || {
                        super::form_runtime::derive_local_work_dir_from_name(
                            &fields.name,
                            std::path::Path::new("/tmp"),
                        )
                    },
                    |repository| {
                        if repository.remote.enabled {
                            super::form_runtime::derive_remote_work_dir_from_name(
                                &fields.name,
                                &repository.base_dir.to_string_lossy(),
                            )
                        } else {
                            super::form_runtime::derive_local_work_dir_from_name(
                                &fields.name,
                                &repository.base_dir,
                            )
                        }
                    },
                );
        }
    }
}

#[cfg(test)]
#[path = "form_ops_remote_work_dir_tests.rs"]
mod remote_work_dir_tests;

#[cfg(test)]
#[path = "form_ops_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "form_ops_issue266_tests.rs"]
mod issue266_tests;

#[cfg(test)]
#[path = "form_ops_issue369_tests.rs"]
mod issue369_tests;

#[cfg(test)]
#[path = "form_ops_issue403_tests.rs"]
mod issue403_tests;
