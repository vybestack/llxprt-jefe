//! Form input handling: character insertion, deletion, cursor movement, field
//! navigation, checkbox toggling, and form submission logic.

use crate::domain::{
    Agent, AgentStatus, RemoteRepositorySettings, Repository, RepositoryId, SandboxEngine,
};
use tracing::warn;

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus,
};
use super::util::{
    delete_char_at, delete_char_before, expand_tilde, generate_id, insert_char_at,
    move_cursor_left, move_cursor_right, normalize_llxprt_debug, normalize_profile,
    normalize_sandbox_flags,
};

impl AppState {
    fn handle_agent_shortcut_char(fields: &mut AgentFormFields, c: char) {
        if c == '0' {
            fields.shortcut_slot = None;
        } else if let Some(digit) = c.to_digit(10)
            && (1..=9).contains(&digit)
        {
            fields.shortcut_slot = u8::try_from(digit).ok();
        }
    }

    fn handle_agent_toggle_char(fields: &mut AgentFormFields, focus: AgentFormFocus, c: char) {
        if c != ' ' && c != 'x' && c != 'X' {
            return;
        }

        match focus {
            AgentFormFocus::PassContinue => {
                fields.pass_continue = !fields.pass_continue;
            }
            AgentFormFocus::Sandbox => {
                fields.sandbox_enabled = !fields.sandbox_enabled;
            }
            AgentFormFocus::SandboxEngine => {
                let current =
                    SandboxEngine::from_form_value(&fields.sandbox_engine).unwrap_or_default();
                current
                    .next()
                    .label()
                    .clone_into(&mut fields.sandbox_engine);
            }
            _ => {}
        }
    }

    fn handle_agent_field_char(
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
            AgentFormFocus::Mode => {
                cursor.mode = insert_char_at(&mut fields.mode, cursor.mode, c);
                false
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    insert_char_at(&mut fields.llxprt_debug, cursor.llxprt_debug, c);
                false
            }
            AgentFormFocus::PassContinue
            | AgentFormFocus::Sandbox
            | AgentFormFocus::SandboxEngine => {
                Self::handle_agent_toggle_char(fields, focus, c);
                false
            }
            AgentFormFocus::SandboxFlags => {
                cursor.sandbox_flags =
                    insert_char_at(&mut fields.sandbox_flags, cursor.sandbox_flags, c);
                false
            }
        }
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub(super) fn handle_form_char(&mut self, c: char) {
        let mut refresh_work_dir = false;

        match &mut self.modal {
            ModalState::Search { query } => {
                query.push(c);
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
            } => match focus {
                RepositoryFormFocus::Name => {
                    cursor.name = insert_char_at(&mut fields.name, cursor.name, c);
                }
                RepositoryFormFocus::BaseDir => {
                    cursor.base_dir = insert_char_at(&mut fields.base_dir, cursor.base_dir, c);
                }
                RepositoryFormFocus::DefaultProfile => {
                    cursor.default_profile =
                        insert_char_at(&mut fields.default_profile, cursor.default_profile, c);
                }
                RepositoryFormFocus::LoginUser => {
                    cursor.login_user =
                        insert_char_at(&mut fields.login_user, cursor.login_user, c);
                }
                RepositoryFormFocus::Host => {
                    cursor.host = insert_char_at(&mut fields.host, cursor.host, c);
                }
                RepositoryFormFocus::RunAsUser => {
                    cursor.run_as_user =
                        insert_char_at(&mut fields.run_as_user, cursor.run_as_user, c);
                }
                RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault => {
                    if c == ' ' || c == 'x' || c == 'X' {
                        Self::toggle_repository_checkbox(fields, *focus);
                    }
                }
            },
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                work_dir_manual,
                ..
            } => {
                if *focus == AgentFormFocus::WorkDir {
                    *work_dir_manual = true;
                }

                let touched_name = Self::handle_agent_field_char(fields, cursor, *focus, c);
                if touched_name && !*work_dir_manual {
                    refresh_work_dir = true;
                }
            }
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => {
                let _ = Self::handle_agent_field_char(fields, cursor, *focus, c);
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
            RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault => {}
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
            RepositoryFormFocus::LoginUser => {
                delete_char_at(&mut fields.login_user, cursor.login_user);
            }
            RepositoryFormFocus::Host => {
                delete_char_at(&mut fields.host, cursor.host);
            }
            RepositoryFormFocus::RunAsUser => {
                delete_char_at(&mut fields.run_as_user, cursor.run_as_user);
            }
            RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault => {}
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
            AgentFormFocus::Mode => {
                cursor.mode = delete_char_before(&mut fields.mode, cursor.mode);
            }
            AgentFormFocus::LlxprtDebug => {
                cursor.llxprt_debug =
                    delete_char_before(&mut fields.llxprt_debug, cursor.llxprt_debug);
            }
            AgentFormFocus::PassContinue
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
            | ModalState::EditRepository { focus, cursor, .. } => match focus {
                RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault => {}
                RepositoryFormFocus::Name => {
                    cursor.name = move_cursor_left(cursor.name);
                }
                RepositoryFormFocus::BaseDir => {
                    cursor.base_dir = move_cursor_left(cursor.base_dir);
                }
                RepositoryFormFocus::DefaultProfile => {
                    cursor.default_profile = move_cursor_left(cursor.default_profile);
                }
                RepositoryFormFocus::LoginUser => {
                    cursor.login_user = move_cursor_left(cursor.login_user);
                }
                RepositoryFormFocus::Host => {
                    cursor.host = move_cursor_left(cursor.host);
                }
                RepositoryFormFocus::RunAsUser => {
                    cursor.run_as_user = move_cursor_left(cursor.run_as_user);
                }
            },
            ModalState::NewAgent { focus, cursor, .. }
            | ModalState::EditAgent { focus, cursor, .. } => match focus {
                AgentFormFocus::Shortcut
                | AgentFormFocus::PassContinue
                | AgentFormFocus::Sandbox
                | AgentFormFocus::SandboxEngine => {}
                AgentFormFocus::Name => {
                    cursor.name = move_cursor_left(cursor.name);
                }
                AgentFormFocus::Description => {
                    cursor.description = move_cursor_left(cursor.description);
                }
                AgentFormFocus::WorkDir => {
                    cursor.work_dir = move_cursor_left(cursor.work_dir);
                }
                AgentFormFocus::Profile => {
                    cursor.profile = move_cursor_left(cursor.profile);
                }
                AgentFormFocus::Mode => {
                    cursor.mode = move_cursor_left(cursor.mode);
                }
                AgentFormFocus::LlxprtDebug => {
                    cursor.llxprt_debug = move_cursor_left(cursor.llxprt_debug);
                }
                AgentFormFocus::SandboxFlags => {
                    cursor.sandbox_flags = move_cursor_left(cursor.sandbox_flags);
                }
            },
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)]
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
            } => match focus {
                RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault => {}
                RepositoryFormFocus::Name => {
                    cursor.name = move_cursor_right(&fields.name, cursor.name);
                }
                RepositoryFormFocus::BaseDir => {
                    cursor.base_dir = move_cursor_right(&fields.base_dir, cursor.base_dir);
                }
                RepositoryFormFocus::DefaultProfile => {
                    cursor.default_profile =
                        move_cursor_right(&fields.default_profile, cursor.default_profile);
                }
                RepositoryFormFocus::LoginUser => {
                    cursor.login_user = move_cursor_right(&fields.login_user, cursor.login_user);
                }
                RepositoryFormFocus::Host => {
                    cursor.host = move_cursor_right(&fields.host, cursor.host);
                }
                RepositoryFormFocus::RunAsUser => {
                    cursor.run_as_user = move_cursor_right(&fields.run_as_user, cursor.run_as_user);
                }
            },
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
            } => match focus {
                AgentFormFocus::Shortcut
                | AgentFormFocus::PassContinue
                | AgentFormFocus::Sandbox
                | AgentFormFocus::SandboxEngine => {}
                AgentFormFocus::Name => {
                    cursor.name = move_cursor_right(&fields.name, cursor.name);
                }
                AgentFormFocus::Description => {
                    cursor.description = move_cursor_right(&fields.description, cursor.description);
                }
                AgentFormFocus::WorkDir => {
                    cursor.work_dir = move_cursor_right(&fields.work_dir, cursor.work_dir);
                }
                AgentFormFocus::Profile => {
                    cursor.profile = move_cursor_right(&fields.profile, cursor.profile);
                }
                AgentFormFocus::Mode => {
                    cursor.mode = move_cursor_right(&fields.mode, cursor.mode);
                }
                AgentFormFocus::LlxprtDebug => {
                    cursor.llxprt_debug =
                        move_cursor_right(&fields.llxprt_debug, cursor.llxprt_debug);
                }
                AgentFormFocus::SandboxFlags => {
                    cursor.sandbox_flags =
                        move_cursor_right(&fields.sandbox_flags, cursor.sandbox_flags);
                }
            },
            _ => {}
        }
    }

    pub(super) fn handle_form_next_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, .. } | ModalState::EditRepository { focus, .. } => {
                *focus = focus.next();
            }
            ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                *focus = focus.next();
            }
            _ => {}
        }
    }

    pub(super) fn handle_form_prev_field(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, .. } | ModalState::EditRepository { focus, .. } => {
                *focus = focus.prev();
            }
            ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                *focus = focus.prev();
            }
            _ => {}
        }
    }

    pub(super) fn toggle_repository_checkbox(
        fields: &mut RepositoryFormFields,
        focus: RepositoryFormFocus,
    ) {
        match focus {
            RepositoryFormFocus::RemoteEnabled => {
                fields.remote_enabled = !fields.remote_enabled;
            }
            RepositoryFormFocus::SetupEnvDefault => {
                fields.setup_env_default = !fields.setup_env_default;
            }
            RepositoryFormFocus::Name
            | RepositoryFormFocus::BaseDir
            | RepositoryFormFocus::DefaultProfile
            | RepositoryFormFocus::LoginUser
            | RepositoryFormFocus::Host
            | RepositoryFormFocus::RunAsUser => {}
        }
    }

    pub(super) fn handle_form_toggle_checkbox(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { fields, focus, .. }
            | ModalState::EditRepository { fields, focus, .. } => {
                Self::toggle_repository_checkbox(fields, *focus);
            }
            ModalState::NewAgent { fields, focus, .. }
            | ModalState::EditAgent { fields, focus, .. } => match focus {
                AgentFormFocus::PassContinue => {
                    fields.pass_continue = !fields.pass_continue;
                }
                AgentFormFocus::Shortcut => {
                    let next = match fields.shortcut_slot {
                        None => Some(1),
                        Some(9) => None,
                        Some(slot) => Some(slot + 1),
                    };
                    fields.shortcut_slot = next;
                }
                AgentFormFocus::Sandbox => {
                    fields.sandbox_enabled = !fields.sandbox_enabled;
                }
                AgentFormFocus::SandboxEngine => {
                    let current =
                        SandboxEngine::from_form_value(&fields.sandbox_engine).unwrap_or_default();
                    current
                        .next()
                        .label()
                        .clone_into(&mut fields.sandbox_engine);
                }
                AgentFormFocus::Name
                | AgentFormFocus::Description
                | AgentFormFocus::WorkDir
                | AgentFormFocus::Profile
                | AgentFormFocus::Mode
                | AgentFormFocus::LlxprtDebug
                | AgentFormFocus::SandboxFlags => {}
            },
            ModalState::ConfirmDeleteAgent {
                delete_work_dir, ..
            } => {
                *delete_work_dir = !*delete_work_dir;
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

            let slug = fields
                .name
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                // Agent names map to a single directory segment under base_dir;
                // slash is intentionally excluded so users cannot create nested
                // paths via the name field. Use the work_dir field for custom
                // nested paths when needed.
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>();

            fields.work_dir = if slug.is_empty() {
                base_dir
            } else {
                let base_dir = base_dir.trim_end_matches('/');
                format!("{base_dir}/{slug}")
            };
        }
    }

    pub(super) fn remote_settings_from_fields(
        fields: &RepositoryFormFields,
    ) -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled: fields.remote_enabled,
            login_user: fields.login_user.trim().to_owned(),
            host: fields.host.trim().to_owned(),
            run_as_user: fields.run_as_user.trim().to_owned(),
            setup_env_default: fields.setup_env_default,
        }
    }

    pub(super) fn create_repository_from_fields(
        fields: &RepositoryFormFields,
    ) -> Option<Repository> {
        if fields.name.is_empty() {
            return None;
        }

        let slug = fields
            .name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();

        let base_dir = if fields.base_dir.is_empty() {
            format!("/tmp/{slug}")
        } else if fields.remote_enabled {
            fields.base_dir.clone()
        } else {
            expand_tilde(&fields.base_dir)
        };

        if !fields.remote_enabled {
            if let Err(e) = std::fs::create_dir_all(&base_dir) {
                warn!(
                    base_dir = %base_dir,
                    error = %e,
                    "could not create local repository base directory"
                );
            }
        }

        Some(Repository {
            id: RepositoryId(generate_id("repo")),
            name: fields.name.clone(),
            slug,
            base_dir: std::path::PathBuf::from(&base_dir),
            default_profile: normalize_profile(&fields.default_profile),
            remote: Self::remote_settings_from_fields(fields),
            agent_ids: Vec::new(),
        })
    }

    pub(super) fn update_repository_from_fields(
        repo: &mut Repository,
        fields: &RepositoryFormFields,
    ) {
        repo.name.clone_from(&fields.name);
        repo.slug = fields
            .name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();

        if !fields.base_dir.is_empty() {
            repo.base_dir = if fields.remote_enabled {
                std::path::PathBuf::from(&fields.base_dir)
            } else {
                std::path::PathBuf::from(expand_tilde(&fields.base_dir))
            };
        }

        repo.default_profile = normalize_profile(&fields.default_profile);
        repo.remote = Self::remote_settings_from_fields(fields);
    }

    pub(super) fn create_agent_from_fields(
        repository: &Repository,
        fields: &AgentFormFields,
        next_display_index: usize,
    ) -> Option<Agent> {
        if fields.name.is_empty() {
            return None;
        }

        let work_dir = if repository.remote.enabled {
            fields.work_dir.clone()
        } else {
            expand_tilde(&fields.work_dir)
        };
        if !repository.remote.enabled {
            if let Err(e) = std::fs::create_dir_all(&work_dir) {
                warn!(
                    work_dir = %work_dir,
                    error = %e,
                    "could not create local agent work directory"
                );
            }
        }

        let mode_flags: Vec<String> = if fields.mode.trim().is_empty() {
            vec!["--yolo".to_owned()]
        } else {
            fields.mode.split_whitespace().map(String::from).collect()
        };

        let sandbox_engine =
            SandboxEngine::from_form_value(&fields.sandbox_engine).unwrap_or_default();

        Some(Agent {
            id: crate::domain::AgentId(generate_id("agent")),
            display_id: format!("#{next_display_index}"),
            repository_id: repository.id.clone(),
            shortcut_slot: fields.shortcut_slot,
            name: fields.name.clone(),
            description: fields.description.clone(),
            work_dir: std::path::PathBuf::from(&work_dir),
            profile: normalize_profile(&fields.profile),
            mode_flags,
            llxprt_debug: normalize_llxprt_debug(&fields.llxprt_debug),
            pass_continue: fields.pass_continue,
            sandbox_enabled: fields.sandbox_enabled,
            sandbox_engine,
            sandbox_flags: normalize_sandbox_flags(&fields.sandbox_flags),
            status: AgentStatus::Running,
            runtime_binding: None,
        })
    }

    pub(super) fn update_agent_from_fields(
        agent: &mut Agent,
        repository: &Repository,
        fields: &AgentFormFields,
    ) {
        agent.name.clone_from(&fields.name);
        agent.shortcut_slot = fields.shortcut_slot;
        agent.description.clone_from(&fields.description);

        if !fields.work_dir.is_empty() {
            let new_dir = if repository.remote.enabled {
                fields.work_dir.clone()
            } else {
                expand_tilde(&fields.work_dir)
            };
            if !repository.remote.enabled && new_dir != agent.work_dir.to_string_lossy() {
                if let Err(e) = std::fs::create_dir_all(&new_dir) {
                    warn!(
                        work_dir = %new_dir,
                        error = %e,
                        "could not create updated local agent work directory"
                    );
                }
            }
            agent.work_dir = std::path::PathBuf::from(&new_dir);
        }

        agent.profile = normalize_profile(&fields.profile);
        agent.mode_flags = if fields.mode.trim().is_empty() {
            vec!["--yolo".to_owned()]
        } else {
            fields.mode.split_whitespace().map(String::from).collect()
        };
        agent.llxprt_debug = normalize_llxprt_debug(&fields.llxprt_debug);
        agent.pass_continue = fields.pass_continue;
        agent.sandbox_enabled = fields.sandbox_enabled;
        agent.sandbox_engine =
            SandboxEngine::from_form_value(&fields.sandbox_engine).unwrap_or_default();
        agent.sandbox_flags = normalize_sandbox_flags(&fields.sandbox_flags);
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
                if fields.name.is_empty() {
                    return;
                }

                if let Some(repo) = self.repositories.iter_mut().find(|r| r.id == id) {
                    Self::update_repository_from_fields(repo, &fields);
                }
                self.modal = ModalState::None;
            }
            ModalState::NewAgent {
                repository_id,
                fields,
                ..
            } => {
                let next_display_index = self.agents.len() + 1;
                if let Some(repository) = self.repository_by_id(&repository_id).cloned()
                    && let Some(agent) =
                        Self::create_agent_from_fields(&repository, &fields, next_display_index)
                {
                    self.enforce_shortcut_uniqueness(&agent.id, agent.shortcut_slot);
                    self.agents.push(agent);
                    self.selected_agent_index = Some(self.agents.len() - 1);
                    self.remember_selected_agent_for_current_repo();
                    self.modal = ModalState::None;
                }
            }
            ModalState::EditAgent { id, fields, .. } => {
                if fields.name.is_empty() {
                    return;
                }

                self.enforce_shortcut_uniqueness(&id, fields.shortcut_slot);
                let repository = self.repository_for_agent(&id).cloned();
                if let Some(repository) = repository
                    && let Some(agent) = self.agents.iter_mut().find(|a| a.id == id)
                {
                    Self::update_agent_from_fields(agent, &repository, &fields);
                }
                self.remember_selected_agent_for_current_repo();
                self.modal = ModalState::None;
            }
            _ => {
                self.modal = ModalState::None;
            }
        }
    }
}

#[cfg(test)]
#[path = "form_ops_tests.rs"]
mod tests;
