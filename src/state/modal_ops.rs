//! Modal and repository-agent message dispatch.
//!
//! Extracted from the main reducer to keep `mod.rs` within the source-file
//! length limit. These methods open/close/edit modal forms and handle
//! repository/agent-related UI messages.

use crate::domain::{
    AgentId, AgentKind, DEFAULT_SANDBOX_FLAGS, Repository, RepositoryId, SandboxEngine,
};
use crate::messages::{ModalMessage, RepositoryAgentMessage};

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ConfirmFocus, ModalState,
    RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};

#[derive(Default)]
struct NewAgentRepositoryDefaults {
    base_dir: String,
    profile: String,
    code_puppy_model: String,
    agent_kind: AgentKind,
    remote_enabled: bool,
}

fn new_agent_repository_defaults(
    repositories: &[Repository],
    repository_id: &RepositoryId,
) -> NewAgentRepositoryDefaults {
    repositories
        .iter()
        .find(|repository| repository.id == *repository_id)
        .map(|repository| NewAgentRepositoryDefaults {
            base_dir: repository.base_dir.to_string_lossy().into_owned(),
            profile: repository.default_profile.clone(),
            code_puppy_model: repository.default_code_puppy_model.clone(),
            agent_kind: repository.default_agent_kind,
            remote_enabled: repository.remote.enabled,
        })
        .unwrap_or_default()
}

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
            ModalMessage::ConfirmCycleFocus => self.cycle_confirm_focus(),
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
                self.modal = ModalState::ConfirmDeleteRepository {
                    id,
                    confirm_focus: ConfirmFocus::Cancel,
                };
            }
            RepositoryAgentMessage::OpenNewAgent(repository_id) => {
                self.open_new_agent_modal(repository_id);
            }
            RepositoryAgentMessage::OpenEditAgent(id) => self.open_edit_agent_modal(id),
            RepositoryAgentMessage::OpenDeleteAgent(id) => {
                self.modal = ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir: false,
                    confirm_focus: ConfirmFocus::Cancel,
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
                default_code_puppy_yolo: r.default_code_puppy_yolo.unwrap_or(false),
                default_agent_kind: r.default_agent_kind.label().to_owned(),
                github_repo: r.github_repo.clone(),
                github_issue_pr_repo: r.github_issue_pr_repo.clone(),
                remote_enabled: r.remote.enabled,
                login_user: r.remote.login_user.clone(),
                host: r.remote.host.clone(),
                run_as_user: r.remote.run_as_user.clone(),
                setup_env_default: r.remote.setup_env_default,
                transient_agent_dir: r.transient_agent_dir.to_string_lossy().into_owned(),
                transient_max_concurrent: r.transient_max_concurrent.to_string(),
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
                github_issue_pr_repo: fields.github_issue_pr_repo.chars().count(),
                login_user: fields.login_user.chars().count(),
                host: fields.host.chars().count(),
                run_as_user: fields.run_as_user.chars().count(),
                transient_agent_dir: fields.transient_agent_dir.chars().count(),
                transient_max_concurrent: fields.transient_max_concurrent.chars().count(),
            },
            fields,
            focus: RepositoryFormFocus::default(),
        };
    }

    fn open_new_agent_modal(&mut self, repository_id: RepositoryId) {
        let NewAgentRepositoryDefaults {
            base_dir,
            profile: default_profile,
            code_puppy_model: default_code_puppy_model,
            agent_kind: repo_default_kind,
            remote_enabled,
        } = new_agent_repository_defaults(&self.repositories, &repository_id);

        // Remote repositories trust their configured runtime; local ones must
        // fall back when that runtime is not installed.
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
        let code_puppy_model_len = default_code_puppy_model.chars().count();

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
                code_puppy_model: default_code_puppy_model,
                code_puppy_yolo: false,
                code_puppy_quick_resume: crate::domain::QuickResume::default(),
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
                code_puppy_model: code_puppy_model_len,
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
                code_puppy_quick_resume: a.code_puppy_quick_resume.into(),
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
            delete_work_dir, ..
        } = &mut self.modal
        {
            *delete_work_dir = !*delete_work_dir;
        }
    }

    /// Toggle confirm-dialog button focus between Cancel and Confirm (issue #228).
    fn cycle_confirm_focus(&mut self) {
        let next = match self.current_confirm_focus() {
            Some(ConfirmFocus::Cancel) => ConfirmFocus::Confirm,
            Some(ConfirmFocus::Confirm) => ConfirmFocus::Cancel,
            None => return,
        };
        self.set_confirm_focus(next);
    }

    /// Read the confirm focus from whichever confirm variant is active.
    /// Returns `None` for non-confirm modals.
    #[must_use]
    pub fn current_confirm_focus(&self) -> Option<ConfirmFocus> {
        match &self.modal {
            ModalState::ConfirmDeleteAgent { confirm_focus, .. }
            | ModalState::ConfirmDeleteRepository { confirm_focus, .. }
            | ModalState::ConfirmKillAgent { confirm_focus, .. }
            | ModalState::PreflightPrompt { confirm_focus, .. }
            | ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. }
            | ModalState::ConfirmIssueOriginMismatch { confirm_focus, .. } => Some(*confirm_focus),
            _ => None,
        }
    }

    /// Replace the confirm focus on the active confirm variant, preserving all
    /// other fields. No-op for non-confirm modals (issue #228).
    fn set_confirm_focus(&mut self, focus: ConfirmFocus) {
        match &mut self.modal {
            ModalState::ConfirmDeleteAgent { confirm_focus, .. }
            | ModalState::ConfirmDeleteRepository { confirm_focus, .. }
            | ModalState::ConfirmKillAgent { confirm_focus, .. }
            | ModalState::PreflightPrompt { confirm_focus, .. }
            | ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. }
            | ModalState::ConfirmIssueOriginMismatch { confirm_focus, .. } => {
                *confirm_focus = focus;
            }
            _ => {}
        }
    }
}
