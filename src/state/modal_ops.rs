//! Modal and repository-agent message dispatch.
//!
//! Extracted from the main reducer to keep `mod.rs` within the source-file
//! length limit. These methods open/close/edit modal forms and handle
//! repository/agent-related UI messages.

use crate::domain::{AgentId, DEFAULT_SANDBOX_FLAGS, RepositoryId, SandboxEngine};
use crate::messages::{ModalMessage, RepositoryAgentMessage};

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus,
};

impl AppState {
    pub(super) fn apply_modal_message(&mut self, message: ModalMessage) {
        match message {
            ModalMessage::OpenHelp => self.modal = ModalState::Help,
            ModalMessage::OpenSearch => {
                self.modal = ModalState::Search {
                    query: String::new(),
                };
            }
            ModalMessage::CloseModal => self.modal = ModalState::None,
            ModalMessage::SubmitForm => self.handle_submit_form(),
            ModalMessage::FormChar(c) => self.handle_form_char(c),
            ModalMessage::FormBackspace => self.handle_form_backspace(),
            ModalMessage::FormDelete => self.handle_form_delete(),
            ModalMessage::FormMoveCursorLeft => self.handle_form_move_cursor_left(),
            ModalMessage::FormMoveCursorRight => self.handle_form_move_cursor_right(),
            ModalMessage::FormNextField => self.handle_form_next_field(),
            ModalMessage::FormPrevField => self.handle_form_prev_field(),
            ModalMessage::FormToggleCheckbox => self.handle_form_toggle_checkbox(),
        }
    }

    pub(super) fn apply_repository_agent_message(&mut self, message: RepositoryAgentMessage) {
        match message {
            RepositoryAgentMessage::OpenNewRepository => self.open_new_repository_modal(),
            RepositoryAgentMessage::OpenEditRepository(id) => self.open_edit_repository_modal(id),
            RepositoryAgentMessage::OpenDeleteRepository(id) => {
                self.modal = ModalState::ConfirmDeleteRepository { id };
            }
            RepositoryAgentMessage::OpenNewAgent(repository_id) => {
                self.open_new_agent_modal(repository_id);
            }
            RepositoryAgentMessage::OpenEditAgent(id) => self.open_edit_agent_modal(id),
            RepositoryAgentMessage::OpenDeleteAgent(id) => {
                self.modal = ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir: false,
                };
            }
            RepositoryAgentMessage::ToggleDeleteWorkDir => self.toggle_delete_work_dir(),
        }
    }

    fn open_new_repository_modal(&mut self) {
        let default_kind = self
            .installed_agent_kinds
            .first()
            .copied()
            .unwrap_or_default();
        self.modal = ModalState::NewRepository {
            fields: RepositoryFormFields {
                default_agent_kind: default_kind.label().to_owned(),
                ..RepositoryFormFields::default()
            },
            focus: RepositoryFormFocus::default(),
            cursor: RepositoryFormCursor::default(),
        };
    }

    fn open_edit_repository_modal(&mut self, id: RepositoryId) {
        let fields = self
            .repositories
            .iter()
            .find(|r| r.id == id)
            .map(|r| RepositoryFormFields {
                name: r.name.clone(),
                base_dir: r.base_dir.to_string_lossy().into_owned(),
                default_profile: r.default_profile.clone(),
                default_code_puppy_model: r.default_code_puppy_model.clone(),
                default_agent_kind: r.default_agent_kind.label().to_owned(),
                github_repo: r.github_repo.clone(),
                remote_enabled: r.remote.enabled,
                login_user: r.remote.login_user.clone(),
                host: r.remote.host.clone(),
                run_as_user: r.remote.run_as_user.clone(),
                setup_env_default: r.remote.setup_env_default,
            })
            .unwrap_or_default();
        self.modal = ModalState::EditRepository {
            id,
            cursor: RepositoryFormCursor {
                name: fields.name.chars().count(),
                base_dir: fields.base_dir.chars().count(),
                default_profile: fields.default_profile.chars().count(),
                default_code_puppy_model: fields.default_code_puppy_model.chars().count(),
                github_repo: fields.github_repo.chars().count(),
                login_user: fields.login_user.chars().count(),
                host: fields.host.chars().count(),
                run_as_user: fields.run_as_user.chars().count(),
            },
            fields,
            focus: RepositoryFormFocus::default(),
        };
    }

    fn open_new_agent_modal(&mut self, repository_id: RepositoryId) {
        let (base_dir, default_profile, repo_default_kind, remote_enabled) = self
            .repositories
            .iter()
            .find(|r| r.id == repository_id)
            .map(|r| {
                (
                    r.base_dir.to_string_lossy().into_owned(),
                    r.default_profile.clone(),
                    r.default_agent_kind,
                    r.remote.enabled,
                )
            })
            .unwrap_or_default();

        // Initialize the agent kind to the repository's default when it is
        // installed (or the repo is remote — remote resolution is
        // authoritative). Otherwise fall back to the first installed kind.
        let agent_kind =
            if remote_enabled || self.installed_agent_kinds.contains(&repo_default_kind) {
                repo_default_kind
            } else {
                self.installed_agent_kinds
                    .first()
                    .copied()
                    .unwrap_or_default()
            };

        let work_dir_len = base_dir.chars().count();
        let profile_len = default_profile.chars().count();

        // Default mode string: LLxprt agents launch with `--yolo`; others
        // start empty. Computed once and reused for both the field value and
        // the cursor length to keep them in sync.
        let default_mode = if agent_kind == crate::domain::AgentKind::Llxprt {
            "--yolo"
        } else {
            ""
        };

        self.modal = ModalState::NewAgent {
            repository_id,
            fields: AgentFormFields {
                shortcut_slot: self.first_unused_shortcut_slot(None),
                name: String::new(),
                description: String::new(),
                work_dir: base_dir,
                profile: default_profile,
                code_puppy_model: String::new(),
                code_puppy_yolo: false,
                agent_kind: agent_kind.label().to_owned(),
                mode: default_mode.to_owned(),
                llxprt_debug: String::new(),
                pass_continue: true,
                sandbox_enabled: false,
                sandbox_engine: SandboxEngine::Podman.label().to_owned(),
                sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            },
            cursor: AgentFormCursor {
                work_dir: work_dir_len,
                profile: profile_len,
                code_puppy_model: 0,
                mode: default_mode.chars().count(),
                sandbox_flags: DEFAULT_SANDBOX_FLAGS.chars().count(),
                ..AgentFormCursor::default()
            },
            focus: AgentFormFocus::default(),
            work_dir_manual: false,
        };
    }

    fn open_edit_agent_modal(&mut self, id: AgentId) {
        let fields = self
            .agents
            .iter()
            .find(|a| a.id == id)
            .map(|a| AgentFormFields {
                shortcut_slot: a.shortcut_slot,
                name: a.name.clone(),
                description: a.description.clone(),
                work_dir: a.work_dir.to_string_lossy().into_owned(),
                profile: a.profile.clone(),
                code_puppy_model: a.code_puppy_model.clone(),
                code_puppy_yolo: a.code_puppy_yolo.unwrap_or(false),
                agent_kind: a.agent_kind.label().to_owned(),
                mode: a.mode_flags.join(" "),
                llxprt_debug: a.llxprt_debug.clone(),
                pass_continue: a.pass_continue,
                sandbox_enabled: a.sandbox_enabled,
                sandbox_engine: a.sandbox_engine.label().to_owned(),
                sandbox_flags: a.sandbox_flags.clone(),
            })
            .unwrap_or_default();
        self.modal = ModalState::EditAgent {
            id,
            cursor: AgentFormCursor {
                name: fields.name.chars().count(),
                description: fields.description.chars().count(),
                work_dir: fields.work_dir.chars().count(),
                profile: fields.profile.chars().count(),
                code_puppy_model: fields.code_puppy_model.chars().count(),
                mode: fields.mode.chars().count(),
                llxprt_debug: fields.llxprt_debug.chars().count(),
                sandbox_flags: fields.sandbox_flags.chars().count(),
            },
            fields,
            focus: AgentFormFocus::default(),
        };
    }

    fn toggle_delete_work_dir(&mut self) {
        if let ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
        } = self.modal.clone()
        {
            self.modal = ModalState::ConfirmDeleteAgent {
                id,
                delete_work_dir: !delete_work_dir,
            };
        }
    }
}
