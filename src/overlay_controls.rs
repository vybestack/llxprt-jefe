//! Deterministic host-control models for host-owned screen overlays.

use crate::domain::action_registry::{ActionId, InternalActionId};
use crate::domain::plugin::field::{Field, InternalField};
use crate::domain::{InternalId, TypedValue};
use crate::host_controls::{
    ControlAction, ControlIntent, ControlKind, HostControlRow, PanelHitTarget, control_intent_body,
    project_control_body,
};
use crate::runtime::provider::protocol::{
    Affordance, DetailBody, ErrorBody, FormBody, Id, PanelBody, ProgressBody, StatusBody,
    StatusRow, StatusRowState, TypedMap,
};
use crate::state::provider_view::{ProviderRowStatus, ProviderViewMode, ProviderViewProjection};
use crate::state::{
    AppState, ConfirmFocus, ModalState, RepositoryFormCursor, RepositoryFormFields,
    RepositoryFormFocus,
};

pub const HELP_FOOTER: &str = "Esc/? close | Up/Down scroll";
pub const CONFIRMATION_FOOTER: &str = "Enter confirm | Esc cancel";
pub const REPOSITORY_FORM_FOOTER: &str = "Tab/Down next | Shift+Tab/Up prev | Left/Right move cursor | Space toggles | Enter submit | Esc cancel";

/// Exact dimensions shared by host-overlay projection, drawing, input, and selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostOverlayLayout {
    pub width: u16,
    pub height: u16,
    pub content_width: usize,
    pub viewport_rows: usize,
}

impl HostOverlayLayout {
    #[must_use]
    pub fn help(cols: u16, rows: u16) -> Self {
        Self::bounded(cols, rows, 60, rows)
    }

    #[must_use]
    pub fn confirmation(cols: u16, rows: u16) -> Self {
        Self::bounded(cols, rows, 50, 10)
    }

    #[must_use]
    pub fn provider(cols: u16, rows: u16) -> Self {
        Self::bounded(cols, rows, 60, rows)
    }

    /// The repository form keeps its legacy full-terminal footprint: it owns
    /// the screen while open, so it is bounded only by the terminal itself.
    #[must_use]
    pub fn form(cols: u16, rows: u16) -> Self {
        Self::bounded(cols, rows, cols, rows)
    }

    fn bounded(cols: u16, rows: u16, max_width: u16, max_height: u16) -> Self {
        let width = cols.min(max_width);
        let height = rows.min(max_height);
        Self {
            width,
            height,
            content_width: usize::from(width.saturating_sub(4)),
            // The overlay shell draws a title row and a footer row inside the
            // same border+padding interior, so the projected window is two rows
            // smaller than the box interior (`height - 6`). Drawing, mouse
            // hit-testing, and selection all consume this one number.
            viewport_rows: usize::from(height.saturating_sub(6)),
        }
    }
}

/// One factory-projected host overlay model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayControlProjection {
    pub kind: ControlKind,
    pub title: String,
    pub rows: Vec<HostControlRow>,
    pub viewport: usize,
    body: PanelBody,
    action_affordances: Vec<Affordance>,
    pub focus_target: Option<Id>,
    form_draft: Option<TypedMap>,
}

impl OverlayControlProjection {
    /// Rendered text rows from this exact typed control projection.
    pub fn text_rows(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|row| row.text.as_str())
    }
}

pub fn overlay_intent(
    projection: &OverlayControlProjection,
    action: ControlAction,
) -> ControlIntent {
    control_intent_body(
        &projection.body,
        &projection.action_affordances,
        None,
        projection.focus_target.as_ref(),
        projection.form_draft.as_ref(),
        action,
    )
}

/// Domain content retained by a generic confirmation payload.
pub struct ConfirmationContent<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub show_delete_work_dir: bool,
    pub delete_work_dir: bool,
    pub focus: ConfirmFocus,
}

pub fn project_help(state: &AppState, width: usize) -> OverlayControlProjection {
    let body = PanelBody::Detail(DetailBody {
        document: state.help_content_lines().join("\n"),
        metadata: Vec::new(),
        actions: Vec::new(),
    });
    let rows = project_control_body(&body, &[], None, None, width);
    OverlayControlProjection {
        kind: ControlKind::Detail,
        title: "Help - Keyboard Shortcuts".to_owned(),
        rows,
        viewport: state.help_scroll_offset(),
        body,
        action_affordances: Vec::new(),
        focus_target: None,
        form_draft: None,
    }
}

pub fn project_search(state: &AppState, width: usize) -> OverlayControlProjection {
    let query = state.search_query().unwrap_or_default();
    let field = Field::internal(InternalField::SearchQuery);
    let mut values = TypedMap::new();
    values.insert(field.id().clone(), TypedValue::String(query.to_owned()));
    project_form("Search", vec![field], values, query.len(), width)
}
pub fn edited_search_query(state: &AppState, query: String, width: usize) -> Option<String> {
    let projection = project_search(state, width);
    let field_id = Id::internal(InternalId::OverlayQuery);
    match overlay_intent(
        &projection,
        ControlAction::EditField {
            field_id,
            value: TypedValue::String(query),
        },
    ) {
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::FieldChanged {
            value: TypedValue::String(query),
            ..
        }) => Some(query),
        _ => None,
    }
}

pub fn search_submission_accepted(state: &AppState, width: usize) -> bool {
    matches!(
        overlay_intent(&project_search(state, width), ControlAction::Activate),
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::Submit { .. })
    )
}

pub fn project_confirmation(
    content: ConfirmationContent<'_>,
    width: usize,
) -> OverlayControlProjection {
    let decision = match content.focus {
        ConfirmFocus::Cancel => "Cancel",
        ConfirmFocus::Confirm => "Confirm",
    };
    let decision_field = Field::internal(InternalField::ConfirmationDecision);
    let mut fields = vec![decision_field.clone()];
    let mut values = TypedMap::new();
    values.insert(
        decision_field.id().clone(),
        TypedValue::String(decision.to_owned()),
    );
    if content.show_delete_work_dir {
        let delete_field = Field::internal(InternalField::DeleteWorkDir);
        values.insert(
            delete_field.id().clone(),
            TypedValue::Bool(content.delete_work_dir),
        );
        fields.push(delete_field);
    }
    let mut projection = project_form(content.title, fields, values, 0, width);
    prepend_detail_rows(&mut projection.rows, content.message, width);
    projection
}

pub fn confirmation_delete_work_dir_value(
    content: ConfirmationContent<'_>,
    value: bool,
    width: usize,
) -> Option<bool> {
    let projection = project_confirmation(content, width);
    let field_id = Id::internal(InternalId::OverlayDeleteWorkDir);
    match overlay_intent(
        &projection,
        ControlAction::EditField {
            field_id,
            value: TypedValue::Bool(value),
        },
    ) {
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::FieldChanged {
            value: TypedValue::Bool(value),
            ..
        }) => Some(value),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationCommand {
    CycleFocus,
    ChooseCancel,
    ChooseConfirm,
}

pub struct ProviderConfirmationContent<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub confirm_label: &'a str,
    pub focus: ConfirmFocus,
    pub continuation_schema: &'a [Field],
    pub continuation_values: &'a TypedMap,
    pub focused_field: Option<&'a Id>,
}

pub fn project_provider_confirmation(
    content: ProviderConfirmationContent<'_>,
    width: usize,
) -> OverlayControlProjection {
    let decision = match content.focus {
        ConfirmFocus::Cancel => "Cancel",
        ConfirmFocus::Confirm => content.confirm_label,
    };
    let decision_field = Field::internal(InternalField::ConfirmationDecision);
    let mut fields = content.continuation_schema.to_vec();
    fields.push(decision_field.clone());
    let mut values = content.continuation_values.clone();
    values.insert(
        decision_field.id().clone(),
        TypedValue::String(decision.to_owned()),
    );
    let mut projection = project_form(content.title, fields, values, 0, width);
    prepend_detail_rows(&mut projection.rows, content.body, width);
    projection.focus_target = content.focused_field.cloned();
    projection
}

pub fn provider_confirmation_focus(
    state: &AppState,
    action: ControlAction,
    width: usize,
) -> Option<ConfirmFocus> {
    let pending = state.current_provider_confirmation()?;
    let overlays = state.nav.current().overlays();
    let focus = overlays.confirmation_focus()?;
    let empty_values = TypedMap::new();
    let projection = project_provider_confirmation(
        ProviderConfirmationContent {
            title: pending.title(),
            body: pending.body(),
            confirm_label: pending.confirm_label(),
            focus,
            continuation_schema: pending.continuation_schema(),
            continuation_values: overlays.confirmation_values().unwrap_or(&empty_values),
            focused_field: overlays.confirmation_focused_field(),
        },
        width,
    );
    match confirmation_command(&projection, action) {
        Some(ConfirmationCommand::CycleFocus) => Some(match focus {
            ConfirmFocus::Cancel => ConfirmFocus::Confirm,
            ConfirmFocus::Confirm => ConfirmFocus::Cancel,
        }),
        Some(ConfirmationCommand::ChooseCancel) => Some(ConfirmFocus::Cancel),
        Some(ConfirmationCommand::ChooseConfirm) => Some(ConfirmFocus::Confirm),
        None => None,
    }
}

pub fn confirmation_command(
    projection: &OverlayControlProjection,
    action: ControlAction,
) -> Option<ConfirmationCommand> {
    match overlay_intent(projection, action) {
        ControlIntent::Scroll(_) => Some(ConfirmationCommand::CycleFocus),
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::Submit { values }) => {
            let TypedValue::String(decision) =
                values.get(&Id::internal(InternalId::OverlayDecision))?
            else {
                return None;
            };
            // A closed two-choice domain: an unknown decision value must not
            // default to Confirm (which would destroy work); anything that is not
            // Cancel is treated as Confirm only after the closed vocabulary is
            // proven below.
            Some(if decision == "Cancel" {
                ConfirmationCommand::ChooseCancel
            } else {
                ConfirmationCommand::ChooseConfirm
            })
        }
        _ => None,
    }
}

fn provider_status_text(status: &ProviderRowStatus) -> String {
    match status {
        ProviderRowStatus::None => "Ready".to_owned(),
        ProviderRowStatus::Unavailable(reason)
        | ProviderRowStatus::Failed(reason)
        | ProviderRowStatus::GenerationUnavailable(reason)
        | ProviderRowStatus::Completed(reason) => reason.clone(),
        ProviderRowStatus::InProgress(summary) => summary.clone(),
        ProviderRowStatus::Cancelled => "Cancelled".to_owned(),
    }
}

pub fn project_provider_surface(
    projection: &ProviderViewProjection,
    width: usize,
) -> OverlayControlProjection {
    let message = projection
        .rows
        .iter()
        .map(|row| format!("{}  {}", row.label, provider_status_text(&row.status)))
        .collect::<Vec<_>>()
        .join("\n");
    if projection.has_active_request
        && !matches!(
            projection.mode,
            ProviderViewMode::Confirmation { .. } | ProviderViewMode::Small
        )
    {
        project_provider_progress(&message, width)
    } else if !projection.has_active_request
        && !projection.rows.is_empty()
        && !projection
            .rows
            .iter()
            .any(|row| matches!(row.status, ProviderRowStatus::Unavailable(_)))
        && !matches!(
            projection.mode,
            ProviderViewMode::Confirmation { .. } | ProviderViewMode::Small
        )
    {
        project_provider_error(&message, width)
    } else {
        match &projection.mode {
            ProviderViewMode::Confirmation {
                confirm_focus,
                title,
                body,
                confirm_label,
                continuation_schema,
                continuation_values,
                focused_field,
            } => project_provider_confirmation(
                ProviderConfirmationContent {
                    title,
                    body,
                    confirm_label,
                    focus: *confirm_focus,
                    continuation_schema,
                    continuation_values,
                    focused_field: focused_field.as_ref(),
                },
                width,
            ),
            ProviderViewMode::Error { message } => project_provider_error(message, width),
            _ => project_provider_status(projection, width),
        }
    }
}

fn project_provider_progress(message: &str, width: usize) -> OverlayControlProjection {
    let body = PanelBody::Progress(ProgressBody {
        message: message.to_owned(),
        completed: None,
        total: None,
        cancellable: true,
    });
    let rows = project_control_body(&body, &[], None, None, width);
    OverlayControlProjection {
        kind: ControlKind::Progress,
        title: "Provider Action".to_owned(),
        rows,
        viewport: 0,
        body,
        action_affordances: Vec::new(),
        focus_target: None,
        form_draft: None,
    }
}

fn project_provider_error(message: &str, width: usize) -> OverlayControlProjection {
    let retry_action = ActionId::internal(InternalActionId::ProviderRetry);
    let retry_affordance = Id::internal(InternalId::ProviderRetry);
    let body = PanelBody::Error(ErrorBody {
        code: "provider-action".to_owned(),
        message: message.to_owned(),
        retryable: true,
        retry_action: Some(retry_affordance.clone()),
    });
    let action_affordances = vec![Affordance {
        id: retry_affordance,
        label: "Retry".to_owned(),
        action_id: retry_action,
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    }];
    let rows = project_control_body(&body, &action_affordances, None, None, width);
    OverlayControlProjection {
        kind: ControlKind::Error,
        title: "Provider Action".to_owned(),
        rows,
        viewport: 0,
        body,
        action_affordances,
        focus_target: None,
        form_draft: None,
    }
}

/// Consume mouse input owned by the current exact-instance blocking overlay.
///
/// Confirmation and provider-action surfaces have typed click actions at the
/// shell boundary. Help additionally owns wheel scrolling through its shared
/// Detail control. No event reaches the obscured screen control.
pub fn consume_blocking_overlay_mouse(
    state: &mut AppState,
    kind: crossterm::event::MouseEventKind,
    render_cols: u16,
    render_rows: u16,
) -> bool {
    if !state.blocking_overlay_owns_mouse() {
        return false;
    }
    if state.active_overlay_kind() == Some(crate::workbench::OverlayKind::Help) {
        let action = match kind {
            crossterm::event::MouseEventKind::ScrollUp => Some(ControlAction::Previous),
            crossterm::event::MouseEventKind::ScrollDown => Some(ControlAction::Next),
            _ => None,
        };
        if let Some(action) = action
            && let Some((delta, max_scroll)) =
                state.help_control_scroll(action, render_cols, render_rows)
        {
            let current = state.help_scroll_offset();
            let next = if delta.is_negative() {
                current.saturating_sub(delta.unsigned_abs().into())
            } else {
                current
                    .saturating_add(usize::from(delta.unsigned_abs()))
                    .min(max_scroll)
            };
            state.set_help_scroll_offset(next);
        }
    }
    true
}

pub fn provider_retry_accepted(projection: &ProviderViewProjection, width: usize) -> bool {
    matches!(
        overlay_intent(
            &project_provider_surface(projection, width),
            ControlAction::Retry,
        ),
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::Retry)
    )
}

pub fn provider_cancel_accepted(projection: &ProviderViewProjection, width: usize) -> bool {
    matches!(
        overlay_intent(
            &project_provider_surface(projection, width),
            ControlAction::Cancel,
        ),
        ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::Cancel)
    )
}

fn project_provider_status(
    projection: &ProviderViewProjection,
    width: usize,
) -> OverlayControlProjection {
    let mut rows = projection
        .rows
        .iter()
        .map(|row| StatusRow {
            label: format!("{}{}", if row.focused { ">>" } else { "" }, row.label),
            value: provider_status_text(&row.status),
            state: match row.status {
                ProviderRowStatus::Unavailable(_)
                | ProviderRowStatus::Failed(_)
                | ProviderRowStatus::GenerationUnavailable(_) => StatusRowState::Error,
                ProviderRowStatus::Cancelled => StatusRowState::Warning,
                _ => StatusRowState::Normal,
            },
        })
        .collect::<Vec<_>>();
    if let Some(message) = provider_mode_message(projection) {
        rows.insert(
            0,
            StatusRow {
                label: "Status".to_owned(),
                value: message.to_owned(),
                state: StatusRowState::Normal,
            },
        );
    }
    let body = PanelBody::Status(StatusBody { rows });
    let rows = project_control_body(&body, &[], None, None, width);
    OverlayControlProjection {
        kind: ControlKind::Status,
        title: "Provider Action".to_owned(),
        rows,
        viewport: 0,
        body,
        action_affordances: Vec::new(),
        focus_target: None,
        form_draft: None,
    }
}

fn provider_mode_message(projection: &ProviderViewProjection) -> Option<&str> {
    match &projection.mode {
        ProviderViewMode::Focused => Some("Provider controls focused"),
        ProviderViewMode::Unavailable { reason }
        | ProviderViewMode::Recovery { message: reason } => Some(reason),
        ProviderViewMode::Small if projection.has_active_request => {
            Some("Provider action running — press Esc to cancel")
        }
        ProviderViewMode::Small => Some("Provider action"),
        ProviderViewMode::Normal
        | ProviderViewMode::Error { .. }
        | ProviderViewMode::Confirmation { .. } => None,
    }
}

pub fn provider_surface_footer(projection: &ProviderViewProjection) -> &'static str {
    match &projection.mode {
        ProviderViewMode::Confirmation { .. } => "Tab Select   Enter Activate   Esc Cancel",
        ProviderViewMode::Unavailable { .. } => "Esc Close",
        _ if projection
            .rows
            .iter()
            .any(|row| matches!(row.status, ProviderRowStatus::Unavailable(_))) =>
        {
            "Esc Close"
        }
        _ if projection.has_active_request => "Esc Cancel",
        _ => "Enter Retry   Esc Close",
    }
}
fn prepend_detail_rows(rows: &mut Vec<HostControlRow>, message: &str, width: usize) {
    let detail = PanelBody::Detail(DetailBody {
        document: message.to_owned(),
        metadata: Vec::new(),
        actions: Vec::new(),
    });
    let mut prompt_rows = project_control_body(&detail, &[], None, None, width);
    prompt_rows.append(rows);
    *rows = prompt_rows;
}

/// The repository form's declared fields in legacy render order.
///
/// The third slot marks fields whose visibility is gated on the selected agent
/// type; every other field mirrors the legacy renderer and always shows.
const REPOSITORY_FORM_LAYOUT: [(InternalField, RepositoryFormFocus, bool); 21] = [
    (
        InternalField::RepositoryFormName,
        RepositoryFormFocus::Name,
        false,
    ),
    (
        InternalField::RepositoryFormBaseDir,
        RepositoryFormFocus::BaseDir,
        false,
    ),
    (
        InternalField::RepositoryFormDefaultProfile,
        RepositoryFormFocus::DefaultProfile,
        false,
    ),
    (
        InternalField::RepositoryFormDefaultModel,
        RepositoryFormFocus::DefaultCodePuppyModel,
        true,
    ),
    (
        InternalField::RepositoryFormDefaultYolo,
        RepositoryFormFocus::DefaultCodePuppyYolo,
        true,
    ),
    (
        InternalField::RepositoryFormDefaultAgentType,
        RepositoryFormFocus::DefaultAgentType,
        false,
    ),
    (
        InternalField::RepositoryFormDefaultVersion,
        RepositoryFormFocus::DefaultCodePuppyVersion,
        true,
    ),
    (
        InternalField::RepositoryFormDefaultMode,
        RepositoryFormFocus::DefaultLlxprtMode,
        true,
    ),
    (
        InternalField::RepositoryFormDefaultLlxprtVersion,
        RepositoryFormFocus::DefaultLlxprtVersion,
        true,
    ),
    (
        InternalField::RepositoryFormGithubRepo,
        RepositoryFormFocus::GitHubRepo,
        false,
    ),
    (
        InternalField::RepositoryFormIssuePrRepo,
        RepositoryFormFocus::IssuePrRepo,
        false,
    ),
    (
        InternalField::RepositoryFormRemoteEnabled,
        RepositoryFormFocus::RemoteEnabled,
        false,
    ),
    (
        InternalField::RepositoryFormLoginUser,
        RepositoryFormFocus::LoginUser,
        false,
    ),
    (
        InternalField::RepositoryFormHost,
        RepositoryFormFocus::Host,
        false,
    ),
    (
        InternalField::RepositoryFormSshPort,
        RepositoryFormFocus::SshPort,
        false,
    ),
    (
        InternalField::RepositoryFormIdentityFile,
        RepositoryFormFocus::IdentityFile,
        false,
    ),
    (
        InternalField::RepositoryFormSshOptions,
        RepositoryFormFocus::SshOptions,
        false,
    ),
    (
        InternalField::RepositoryFormRunAsUser,
        RepositoryFormFocus::RunAsUser,
        false,
    ),
    (
        InternalField::RepositoryFormSetupEnvDefault,
        RepositoryFormFocus::SetupEnvDefault,
        false,
    ),
    (
        InternalField::RepositoryFormTransientAgentDir,
        RepositoryFormFocus::TransientAgentDir,
        false,
    ),
    (
        InternalField::RepositoryFormTransientMaxConcurrent,
        RepositoryFormFocus::TransientMaxConcurrent,
        false,
    ),
];

fn repository_field_value(fields: &RepositoryFormFields, focus: RepositoryFormFocus) -> TypedValue {
    use RepositoryFormFocus as F;
    let text = match focus {
        F::Name => fields.name.as_str(),
        F::BaseDir => fields.base_dir.as_str(),
        F::DefaultProfile => fields.default_profile.as_str(),
        F::DefaultCodePuppyModel => fields.default_code_puppy_model.as_str(),
        F::DefaultCodePuppyYolo => return TypedValue::Bool(fields.default_code_puppy_yolo),
        F::DefaultAgentType => fields.default_type_id.as_str(),
        F::DefaultCodePuppyVersion => fields.default_code_puppy_version.as_str(),
        F::DefaultLlxprtMode => fields.default_llxprt_mode.as_str(),
        F::DefaultLlxprtVersion => fields.default_llxprt_version.as_str(),
        F::GitHubRepo => fields.github_repo.as_str(),
        F::IssuePrRepo => fields.github_issue_pr_repo.as_str(),
        F::RemoteEnabled => return TypedValue::Bool(fields.remote_enabled),
        F::LoginUser => fields.login_user.as_str(),
        F::Host => fields.host.as_str(),
        F::SshPort => fields.ssh_port.as_str(),
        F::IdentityFile => fields.identity_file.as_str(),
        F::SshOptions => fields.ssh_options.as_str(),
        F::RunAsUser => fields.run_as_user.as_str(),
        F::SetupEnvDefault => return TypedValue::Bool(fields.setup_env_default),
        F::TransientAgentDir => fields.transient_agent_dir.as_str(),
        F::TransientMaxConcurrent => fields.transient_max_concurrent.as_str(),
    };
    TypedValue::String(text.to_owned())
}

/// Raw text and caret offset of a text field; `None` for booleans, which have
/// no caret.
fn repository_field_text(
    fields: &RepositoryFormFields,
    cursor: &RepositoryFormCursor,
    focus: RepositoryFormFocus,
) -> Option<(String, usize)> {
    use RepositoryFormFocus as F;
    let (text, offset) = match focus {
        F::Name => (&fields.name, cursor.name),
        F::BaseDir => (&fields.base_dir, cursor.base_dir),
        F::DefaultProfile => (&fields.default_profile, cursor.default_profile),
        F::DefaultCodePuppyModel => (
            &fields.default_code_puppy_model,
            cursor.default_code_puppy_model,
        ),
        F::DefaultAgentType => (&fields.default_type_id, 0),
        F::DefaultCodePuppyVersion => (
            &fields.default_code_puppy_version,
            cursor.default_code_puppy_version,
        ),
        F::DefaultLlxprtMode => (&fields.default_llxprt_mode, cursor.default_llxprt_mode),
        F::DefaultLlxprtVersion => (
            &fields.default_llxprt_version,
            cursor.default_llxprt_version,
        ),
        F::GitHubRepo => (&fields.github_repo, cursor.github_repo),
        F::IssuePrRepo => (&fields.github_issue_pr_repo, cursor.github_issue_pr_repo),
        F::LoginUser => (&fields.login_user, cursor.login_user),
        F::Host => (&fields.host, cursor.host),
        F::SshPort => (&fields.ssh_port, cursor.ssh_port),
        F::IdentityFile => (&fields.identity_file, cursor.identity_file),
        F::SshOptions => (&fields.ssh_options, cursor.ssh_options),
        F::RunAsUser => (&fields.run_as_user, cursor.run_as_user),
        F::TransientAgentDir => (&fields.transient_agent_dir, cursor.transient_agent_dir),
        F::TransientMaxConcurrent => (
            &fields.transient_max_concurrent,
            cursor.transient_max_concurrent,
        ),
        F::DefaultCodePuppyYolo | F::RemoteEnabled | F::SetupEnvDefault => return None,
    };
    Some((text.clone(), offset))
}

/// Project the open repository form (new or edit) as a shared-shell form
/// control. Visibility follows the legacy renderer: identity and connection
/// fields always show, type-gated defaults follow the selected agent type.
#[must_use]
pub fn project_repository_form(state: &AppState, width: usize) -> Option<OverlayControlProjection> {
    let (title, fields, focus, cursor) = match &state.modal {
        ModalState::NewRepository {
            fields,
            focus,
            cursor,
        } => ("New Repository", fields, focus, cursor),
        ModalState::EditRepository {
            fields,
            focus,
            cursor,
            ..
        } => ("Edit Repository", fields, focus, cursor),
        _ => return None,
    };
    let type_id = crate::state::type_id_from_form_value(&fields.default_type_id);
    let mut declared = Vec::new();
    let mut values = TypedMap::new();
    let mut focused = None;
    for (field, slot, gated) in REPOSITORY_FORM_LAYOUT {
        if gated && !crate::state::is_repository_field_visible(slot, type_id.as_ref()) {
            continue;
        }
        let declaration = Field::internal(field);
        values.insert(
            declaration.id().clone(),
            repository_field_value(fields, slot),
        );
        if slot == *focus {
            focused = Some((
                declaration.id().clone(),
                declaration.label().to_owned(),
                repository_field_text(fields, cursor, slot),
            ));
        }
        declared.push(declaration);
    }
    let mut projection = project_form(title, declared, values, 0, width);
    if let Some((id, label, Some((text, offset)))) = &focused {
        let caret = crate::ui::util::text_with_caret(text, *offset);
        for row in &mut projection.rows {
            if row.target.as_ref() == Some(&PanelHitTarget::Field(id.clone())) {
                row.text = format!("{label}: {caret}");
            }
        }
    }
    if let Some(error) = state.error_message.as_deref() {
        let error_line = format!("Error: {error}");
        prepend_detail_rows(&mut projection.rows, &error_line, width);
    }
    projection.focus_target = focused.map(|(id, _, _)| id);
    Some(projection)
}

fn project_form(
    title: &str,
    fields: Vec<Field>,
    values: TypedMap,
    viewport: usize,
    width: usize,
) -> OverlayControlProjection {
    let submit_action = ActionId::internal(InternalActionId::OverlaySubmit);
    let body = PanelBody::Form(FormBody {
        fields,
        values,
        field_errors: Vec::new(),
        submit_action: submit_action.clone(),
    });
    let affordances = [Affordance {
        id: Id::internal(InternalId::OverlaySubmit),
        label: "Apply".to_owned(),
        action_id: submit_action,
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    }];
    let rows = project_control_body(&body, &affordances, None, None, width);
    OverlayControlProjection {
        kind: ControlKind::Form,
        title: title.to_owned(),
        rows,
        viewport,
        body,
        action_affordances: affordances.into(),
        focus_target: None,
        form_draft: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::transition::TransitionExt;
    use crate::state::{AppEvent, AppState};

    #[test]
    fn help_search_and_confirmation_use_the_closed_host_factories() {
        let state = AppState::new(crate::test_support::published_workbench())
            .apply(AppEvent::OpenHelp)
            .committed_pure();
        let help = project_help(&state, 60);
        assert_eq!(help.kind, ControlKind::Detail);
        assert!(!help.rows.is_empty());
        assert_eq!(
            overlay_intent(&help, ControlAction::Next),
            ControlIntent::Scroll(1)
        );

        let state = state
            .apply(AppEvent::CloseModal)
            .committed_pure()
            .apply(AppEvent::OpenSearch)
            .committed_pure()
            .apply(AppEvent::FormChar('x'))
            .committed_pure();
        let search = project_search(&state, 60);
        assert_eq!(search.kind, ControlKind::Form);
        assert!(search.rows.iter().any(|row| row.text.contains("Filter: x")));

        let confirmation = project_confirmation(
            ConfirmationContent {
                title: "Confirm",
                message: "Proceed?",
                show_delete_work_dir: false,
                delete_work_dir: false,
                focus: ConfirmFocus::Cancel,
            },
            60,
        );
        assert_eq!(confirmation.kind, ControlKind::Form);
        assert!(confirmation.rows.iter().any(|row| row.text == "Proceed?"));
        assert!(
            confirmation
                .rows
                .iter()
                .any(|row| row.text == "Decision: Cancel")
        );
        assert!(matches!(
            overlay_intent(&confirmation, ControlAction::Activate),
            ControlIntent::Event(crate::runtime::provider::protocol::PanelEvent::Submit { .. })
        ));
    }

    #[test]
    fn confirmation_checkbox_and_provider_lifecycle_use_typed_control_intents() {
        let content = ConfirmationContent {
            title: "Delete agent",
            message: "Proceed?",
            show_delete_work_dir: true,
            delete_work_dir: false,
            focus: ConfirmFocus::Cancel,
        };
        assert_eq!(
            confirmation_delete_work_dir_value(content, true, 50),
            Some(true)
        );

        let active = ProviderViewProjection {
            mode: ProviderViewMode::Focused,
            rows: Vec::new(),
            has_active_request: true,
        };
        assert!(provider_cancel_accepted(&active, 60));
        assert!(!provider_retry_accepted(&active, 60));

        let terminal = ProviderViewProjection {
            mode: ProviderViewMode::Error {
                message: "failed".to_owned(),
            },
            rows: Vec::new(),
            has_active_request: false,
        };
        assert!(provider_retry_accepted(&terminal, 60));
        assert!(!provider_cancel_accepted(&terminal, 60));
    }
}
