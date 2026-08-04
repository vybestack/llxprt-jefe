//! Modal and repository-agent message dispatch.
//!
//! Extracted from the main reducer to keep `mod.rs` within the source-file
//! length limit. These methods open/close/edit modal forms and handle
//! repository/agent-related UI messages.

use crate::domain::agent_definition::{AgentDefinition, AgentTypeId, FieldKind, FieldValue};
use crate::domain::canonical_values::typed_field;
use crate::domain::{
    AgentId, DEFAULT_SANDBOX_FLAGS, Repository, RepositoryId, SandboxEngine, TypedMap, TypedValue,
};
use crate::messages::{ModalMessage, RepositoryAgentMessage};

use super::AppState;
use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, ConfirmFocus, ModalState,
    RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};

struct NewAgentRepositoryDefaults {
    base_dir: String,
    type_id: AgentTypeId,
    values: TypedMap,
    remote_enabled: bool,
}

fn new_agent_repository_defaults(
    repositories: &[Repository],
    repository_id: &RepositoryId,
) -> Option<NewAgentRepositoryDefaults> {
    repositories
        .iter()
        .find(|repository| repository.id == *repository_id)
        .map(|repository| NewAgentRepositoryDefaults {
            base_dir: repository.base_dir.to_string_lossy().into_owned(),
            type_id: repository.default_type_id.clone(),
            values: repository.default_values.clone(),
            remote_enabled: repository.remote.enabled,
        })
}

fn new_agent_form(
    defaults: NewAgentRepositoryDefaults,
    type_id: AgentTypeId,
    shortcut_slot: Option<u8>,
) -> (AgentFormFields, AgentFormCursor) {
    let values = if defaults.type_id == type_id {
        defaults.values
    } else {
        default_values_for_type(&type_id)
    };
    let profile = typed_string(&values, "profile");
    let model = typed_string(&values, "model");
    let selector = typed_string(&values, "version_selector");
    let yolo = typed_bool(&values, "yolo").unwrap_or(false);
    let fields = AgentFormFields {
        shortcut_slot,
        work_dir: defaults.base_dir,
        profile,
        code_puppy_model: model,
        code_puppy_version: selector.clone(),
        code_puppy_yolo: yolo,
        code_puppy_quick_resume: typed_bool(&values, "interactive").unwrap_or(false).into(),
        agent_type_id: type_id.as_str().to_owned(),
        llxprt_version: selector,
        mode: if yolo {
            "--yolo".to_owned()
        } else {
            String::new()
        },
        pass_continue: true,
        sandbox_engine: SandboxEngine::Podman.label().to_owned(),
        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
        ..AgentFormFields::default()
    };
    let cursor = AgentFormCursor {
        work_dir: fields.work_dir.chars().count(),
        profile: fields.profile.chars().count(),
        code_puppy_model: fields.code_puppy_model.chars().count(),
        code_puppy_version: fields.code_puppy_version.chars().count(),
        mode: fields.mode.chars().count(),
        llxprt_version: fields.llxprt_version.chars().count(),
        sandbox_flags: fields.sandbox_flags.chars().count(),
        ..AgentFormCursor::default()
    };
    (fields, cursor)
}

fn default_values_for_type(type_id: &AgentTypeId) -> TypedMap {
    super::form_projection::definition_for_type(type_id)
        .map(|definition| values_from_definition(&definition))
        .unwrap_or_default()
}

fn values_from_definition(definition: &AgentDefinition) -> TypedMap {
    let mut values = TypedMap::new();
    for field in definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
    {
        let value = field
            .default
            .clone()
            .unwrap_or_else(|| empty_field_value(field.kind));
        let Ok(key) = crate::domain::Id::parse(&field.id.replace('_', "-")) else {
            continue;
        };
        if let Some(value) = field_value_to_typed(value) {
            values.insert(key, value);
        }
    }
    values
}

fn empty_field_value(kind: FieldKind) -> FieldValue {
    match kind {
        FieldKind::Boolean => FieldValue::Boolean(false),
        FieldKind::OptionalBoolean => FieldValue::OptionalBoolean(None),
        FieldKind::String | FieldKind::Enum => FieldValue::String(String::new()),
        FieldKind::Integer => FieldValue::Integer(0),
        FieldKind::Path => FieldValue::Path(String::new()),
        FieldKind::StringList => FieldValue::StringList(Vec::new()),
    }
}

fn field_value_to_typed(value: FieldValue) -> Option<TypedValue> {
    match value {
        FieldValue::Boolean(value) => Some(TypedValue::Bool(value)),
        FieldValue::OptionalBoolean(value) => value.map(TypedValue::Bool),
        FieldValue::String(value) | FieldValue::Path(value) => Some(TypedValue::String(value)),
        FieldValue::Integer(value) => Some(TypedValue::Integer(value)),
        FieldValue::StringList(values) => Some(TypedValue::List(
            values.into_iter().map(TypedValue::String).collect(),
        )),
    }
}

fn typed_string(values: &TypedMap, field: &str) -> String {
    match typed_field(values, field) {
        Some(TypedValue::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn typed_bool(values: &TypedMap, field: &str) -> Option<bool> {
    match typed_field(values, field) {
        Some(TypedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn sandbox_engine_field(values: &TypedMap) -> String {
    let value = typed_string(values, "sandbox_engine");
    if value.is_empty() {
        return SandboxEngine::default().label().to_owned();
    }
    SandboxEngine::from_form_value(&value).map_or(value, |engine| engine.label().to_owned())
}

fn sandbox_flags_field(values: &TypedMap) -> String {
    let value = typed_string(values, "sandbox_flags");
    if value.is_empty() {
        DEFAULT_SANDBOX_FLAGS.to_owned()
    } else {
        value
    }
}

impl AppState {
    pub(super) fn apply_modal_message(&mut self, message: ModalMessage) {
        match message {
            ModalMessage::OpenHelp => self.modal = ModalState::Help,
            ModalMessage::OpenKeys { recovery } => {
                if let Some(snapshot) = self.action_registry_snapshot.as_ref() {
                    self.modal = ModalState::Keys {
                        editor: Box::new(super::KeysEditorState::from_snapshot(snapshot, recovery)),
                    };
                }
            }
            ModalMessage::Keys(message) => self.apply_keys_message(message),
            ModalMessage::OpenSearch => {
                self.modal = ModalState::Search {
                    query: String::new(),
                };
            }
            ModalMessage::CloseModal => self.close_modal(),
            ModalMessage::SubmitForm => self.handle_submit_form(),
            ModalMessage::ConfirmCycleFocus => self.cycle_confirm_focus(),
            ModalMessage::FormChar(c) => self.handle_form_char(c),
            ModalMessage::FormBackspace => self.handle_form_backspace(),
            ModalMessage::FormDelete => self.handle_form_delete(),
            ModalMessage::FormMoveCursorLeft => self.handle_form_move_cursor_left(),
            ModalMessage::FormMoveCursorRight => self.handle_form_move_cursor_right(),

            ModalMessage::FormMoveCursorStart => self.handle_form_move_cursor_start(),
            ModalMessage::FormMoveCursorEnd => self.handle_form_move_cursor_end(),
            ModalMessage::FormNextField => self.handle_form_next_field(),
            ModalMessage::FormPrevField => self.handle_form_prev_field(),
            ModalMessage::FormToggleCheckbox => self.handle_form_toggle_checkbox(),
        }
    }

    fn apply_keys_message(&mut self, message: crate::messages::KeysEditorMessage) {
        use crate::messages::KeysEditorMessage;
        if matches!(message, KeysEditorMessage::ConfirmDiscard) {
            self.modal = ModalState::None;
            return;
        }
        if let KeysEditorMessage::SaveSucceeded(snapshot) = message {
            self.action_registry_snapshot = Some(snapshot);
            self.modal = ModalState::None;
            return;
        }
        let close_clean = matches!(message, KeysEditorMessage::RequestClose)
            && matches!(&self.modal, ModalState::Keys { editor } if !editor.is_dirty());
        if close_clean {
            self.modal = ModalState::None;
            return;
        }
        if let ModalState::Keys { editor } = &mut self.modal {
            editor.apply(message);
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
            RepositoryAgentMessage::OpenAgentTypeForm(type_id) => {
                self.open_generated_agent_modal(&type_id);
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
            RepositoryAgentMessage::ProbeAgentAvailability(probes) => {
                self.stage_agent_availability_probes(probes);
            }
            RepositoryAgentMessage::ProjectActionAvailability => {
                self.stage_action_availability_projection();
            }
        }
    }

    fn open_new_repository_modal(&mut self) {
        let default_type_id = self
            .available_agent_type_ids
            .first()
            .cloned()
            .unwrap_or_default();
        self.modal = ModalState::NewRepository {
            fields: RepositoryFormFields {
                default_type_id: default_type_id.as_str().to_owned(),
                default_code_puppy_yolo: true,
                default_llxprt_mode: "--yolo".to_owned(),
                ..RepositoryFormFields::default()
            },
            focus: RepositoryFormFocus::default(),
            cursor: RepositoryFormCursor {
                default_llxprt_mode: "--yolo".chars().count(),
                ..RepositoryFormCursor::default()
            },
        };
    }

    fn repository_fields(repository: &crate::domain::Repository) -> RepositoryFormFields {
        let yolo = typed_bool(&repository.default_values, "yolo").unwrap_or(false);
        let selector = typed_string(&repository.default_values, "version_selector");
        RepositoryFormFields {
            name: repository.name.clone(),
            base_dir: repository.base_dir.to_string_lossy().into_owned(),
            default_profile: typed_string(&repository.default_values, "profile"),
            default_code_puppy_model: typed_string(&repository.default_values, "model"),
            default_code_puppy_version: selector.clone(),
            default_code_puppy_yolo: yolo,
            default_llxprt_mode: typed_string(&repository.default_values, "mode_flags"),
            default_llxprt_version: selector,
            default_type_id: repository.default_type_id.as_str().to_owned(),
            github_repo: repository.github_repo.clone(),
            github_issue_pr_repo: repository.github_issue_pr_repo.clone(),
            remote_enabled: repository.remote.enabled,
            login_user: repository.remote.login_user.clone(),
            host: repository.remote.host.clone(),
            ssh_port: repository
                .remote
                .port
                .map_or_else(String::new, |port| port.to_string()),
            identity_file: repository
                .remote
                .identity_file
                .to_string_lossy()
                .into_owned(),
            ssh_options: repository.remote.options.join(" "),
            run_as_user: repository.remote.run_as_user.clone(),
            setup_env_default: repository.remote.setup_env_default,
            transient_agent_dir: repository
                .transient_agent_dir
                .to_string_lossy()
                .into_owned(),
            transient_max_concurrent: repository.transient_max_concurrent.to_string(),
        }
    }

    fn open_edit_repository_modal(&mut self, id: RepositoryId) {
        let fields = self
            .repositories
            .iter()
            .find(|repository| repository.id == id)
            .map(Self::repository_fields)
            .unwrap_or_default();
        self.modal = ModalState::EditRepository {
            id,
            cursor: RepositoryFormCursor {
                name: fields.name.chars().count(),
                base_dir: fields.base_dir.chars().count(),
                default_profile: fields.default_profile.chars().count(),
                default_code_puppy_model: fields.default_code_puppy_model.chars().count(),
                default_code_puppy_version: fields.default_code_puppy_version.chars().count(),
                default_llxprt_mode: fields.default_llxprt_mode.chars().count(),
                default_llxprt_version: fields.default_llxprt_version.chars().count(),
                github_repo: fields.github_repo.chars().count(),
                github_issue_pr_repo: fields.github_issue_pr_repo.chars().count(),
                login_user: fields.login_user.chars().count(),
                host: fields.host.chars().count(),
                ssh_port: fields.ssh_port.chars().count(),
                identity_file: fields.identity_file.chars().count(),
                ssh_options: fields.ssh_options.chars().count(),
                run_as_user: fields.run_as_user.chars().count(),
                transient_agent_dir: fields.transient_agent_dir.chars().count(),
                transient_max_concurrent: fields.transient_max_concurrent.chars().count(),
            },
            fields,
            focus: RepositoryFormFocus::default(),
        };
    }

    fn open_new_agent_modal(&mut self, repository_id: RepositoryId) {
        let Some(defaults) = new_agent_repository_defaults(&self.repositories, &repository_id)
        else {
            return;
        };
        // Remote repositories trust their configured runtime; local ones must
        // fall back when that runtime is not installed.
        let type_id = if defaults.remote_enabled
            || self.available_agent_type_ids.contains(&defaults.type_id)
        {
            defaults.type_id.clone()
        } else {
            self.available_agent_type_ids
                .first()
                .cloned()
                .unwrap_or_else(|| defaults.type_id.clone())
        };
        let (fields, cursor) =
            new_agent_form(defaults, type_id, self.first_unused_shortcut_slot(None));
        self.modal = ModalState::NewAgent {
            repository_id,
            fields,
            cursor,
            focus: AgentFormFocus::default(),
            work_dir_manual: false,
        };
    }

    fn open_generated_agent_modal(
        &mut self,
        type_id: &crate::domain::agent_definition::AgentTypeId,
    ) {
        let Some(observation) = self
            .agent_type_availability
            .iter()
            .find(|observation| observation.type_id() == type_id)
        else {
            return;
        };
        let definition = crate::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id == *type_id);
        let Some(definition) = definition else {
            return;
        };
        let form = super::generated_agent_form::GeneratedAgentForm::from_definition(
            &definition,
            observation.availability(),
        );
        let Ok(form) = form else {
            return;
        };
        self.modal = ModalState::GeneratedAgent {
            type_id: Box::new(type_id.clone()),
            form: Box::new(form),
            return_focus: self.pane_focus,
            return_agent_type_index: self.selected_agent_type_index,
        };
    }

    fn close_modal(&mut self) {
        if let ModalState::GeneratedAgent {
            return_focus,
            return_agent_type_index,
            ..
        } = &self.modal
        {
            self.pane_focus = *return_focus;
            self.selected_agent_type_index = *return_agent_type_index;
        }
        self.modal = ModalState::None;
    }

    fn open_edit_agent_modal(&mut self, id: AgentId) {
        let fields = self
            .agents
            .iter()
            .find(|a| a.id == id)
            .map(|a| {
                let selector = typed_string(&a.values, "version_selector");
                let yolo = typed_bool(&a.values, "yolo").unwrap_or(false);
                AgentFormFields {
                    shortcut_slot: a.shortcut_slot,
                    name: a.name.clone(),
                    description: a.description.clone(),
                    work_dir: a.work_dir.to_string_lossy().into_owned(),
                    profile: typed_string(&a.values, "profile"),
                    code_puppy_model: typed_string(&a.values, "model"),
                    code_puppy_version: selector.clone(),
                    code_puppy_yolo: yolo,
                    code_puppy_quick_resume: typed_bool(&a.values, "interactive")
                        .unwrap_or(false)
                        .into(),
                    agent_type_id: a.type_id.as_str().to_owned(),
                    llxprt_version: selector,
                    mode: if yolo {
                        "--yolo".to_owned()
                    } else {
                        String::new()
                    },
                    llxprt_debug: String::new(),
                    pass_continue: typed_bool(&a.values, "continue").unwrap_or(true),
                    sandbox_enabled: typed_bool(&a.values, "sandbox_enabled").unwrap_or(false),
                    sandbox_engine: sandbox_engine_field(&a.values),
                    sandbox_flags: sandbox_flags_field(&a.values),
                }
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
                code_puppy_version: fields.code_puppy_version.chars().count(),
                mode: fields.mode.chars().count(),
                llxprt_version: fields.llxprt_version.chars().count(),
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
            | ModalState::ConfirmServerLostRecovery { confirm_focus, .. }
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
            | ModalState::ConfirmServerLostRecovery { confirm_focus, .. }
            | ModalState::PreflightPrompt { confirm_focus, .. }
            | ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. }
            | ModalState::ConfirmIssueOriginMismatch { confirm_focus, .. } => {
                *confirm_focus = focus;
            }
            _ => {}
        }
    }
}
