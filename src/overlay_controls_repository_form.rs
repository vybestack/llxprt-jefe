//! Repository form (new/edit) projection onto the shared overlay Form control.
//!
//! The retained projection builds the old operator-facing row text while the
//! typed form body, field targets, caret, visibility, and submit affordance
//! continue to ride [`OverlayControlProjection`].

use crate::domain::action_registry::{ActionId, InternalActionId};
use crate::domain::plugin::field::{Field, InternalField};
use crate::domain::{InternalId, TypedValue};
use crate::host_controls::{
    HostControlRow, HostControlRowStyle, HostControlTitleStyle, PanelHitTarget,
};
use crate::list_viewport::fit_text_to_width;
use crate::overlay_controls::{OverlayControlProjection, bespoke_form_projection};
use crate::runtime::provider::protocol::{Affordance, Id, TypedMap};
use crate::state::{
    AppState, ModalState, RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};
use unicode_width::UnicodeWidthStr;

const LABEL_WIDTH: usize = 16;

/// The repository form's declared fields in reducer traversal order.
///
/// The third slot marks fields whose visibility is gated on the selected agent
/// type; every other field mirrors the old renderer and always shows.
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
        InternalField::RepositoryFormDefaultAgentType,
        RepositoryFormFocus::DefaultAgentType,
        false,
    ),
    (
        InternalField::RepositoryFormDefaultYolo,
        RepositoryFormFocus::DefaultCodePuppyYolo,
        true,
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

/// Raw text and caret offset of an editable text field; `None` for booleans
/// and the cycle-only default-agent row, which never shows a caret.
fn repository_field_text<'a>(
    fields: &'a RepositoryFormFields,
    cursor: &RepositoryFormCursor,
    focus: RepositoryFormFocus,
) -> Option<(&'a str, usize)> {
    use RepositoryFormFocus as F;
    match focus {
        F::Name => Some((&fields.name, cursor.name)),
        F::BaseDir => Some((&fields.base_dir, cursor.base_dir)),
        F::DefaultProfile => Some((&fields.default_profile, cursor.default_profile)),
        F::DefaultCodePuppyModel => Some((
            &fields.default_code_puppy_model,
            cursor.default_code_puppy_model,
        )),
        F::DefaultCodePuppyVersion => Some((
            &fields.default_code_puppy_version,
            cursor.default_code_puppy_version,
        )),
        F::DefaultLlxprtMode => Some((&fields.default_llxprt_mode, cursor.default_llxprt_mode)),
        F::DefaultLlxprtVersion => Some((
            &fields.default_llxprt_version,
            cursor.default_llxprt_version,
        )),
        F::GitHubRepo => Some((&fields.github_repo, cursor.github_repo)),
        F::IssuePrRepo => Some((&fields.github_issue_pr_repo, cursor.github_issue_pr_repo)),
        F::LoginUser => Some((&fields.login_user, cursor.login_user)),
        F::Host => Some((&fields.host, cursor.host)),
        F::SshPort => Some((&fields.ssh_port, cursor.ssh_port)),
        F::IdentityFile => Some((&fields.identity_file, cursor.identity_file)),
        F::SshOptions => Some((&fields.ssh_options, cursor.ssh_options)),
        F::RunAsUser => Some((&fields.run_as_user, cursor.run_as_user)),
        F::TransientAgentDir => Some((&fields.transient_agent_dir, cursor.transient_agent_dir)),
        F::TransientMaxConcurrent => Some((
            &fields.transient_max_concurrent,
            cursor.transient_max_concurrent,
        )),
        F::DefaultAgentType | F::DefaultCodePuppyYolo | F::RemoteEnabled | F::SetupEnvDefault => {
            None
        }
    }
}

fn display_label(field: InternalField, declared_label: &str) -> &str {
    match field {
        InternalField::RepositoryFormDefaultVersion
        | InternalField::RepositoryFormDefaultLlxprtVersion => "Default Version",
        InternalField::RepositoryFormSshOptions => "SSH Options (space-separated)",
        _ => declared_label,
    }
}

fn checkbox(value: bool) -> &'static str {
    if value { "x" } else { " " }
}

fn display_value(
    fields: &RepositoryFormFields,
    cursor: &RepositoryFormCursor,
    slot: RepositoryFormFocus,
    focused: bool,
) -> String {
    use RepositoryFormFocus as F;
    match slot {
        F::DefaultCodePuppyYolo => checkbox(fields.default_code_puppy_yolo).to_owned(),
        F::RemoteEnabled => checkbox(fields.remote_enabled).to_owned(),
        F::SetupEnvDefault => checkbox(fields.setup_env_default).to_owned(),
        F::DefaultAgentType => fields.default_type_id.clone(),
        _ => {
            let Some((text, offset)) = repository_field_text(fields, cursor, slot) else {
                return String::new();
            };
            if focused {
                crate::ui::util::text_with_caret(text, offset)
            } else {
                text.to_owned()
            }
        }
    }
}

fn field_hint(
    state: &AppState,
    fields: &RepositoryFormFields,
    slot: RepositoryFormFocus,
) -> Option<String> {
    use RepositoryFormFocus as F;
    match slot {
        F::DefaultAgentType => Some(crate::state::effective_types_hint(
            &crate::state::effective_agent_type_ids(
                &state.available_agent_type_ids,
                fields.remote_enabled,
            ),
        )),
        F::DefaultCodePuppyYolo | F::RemoteEnabled | F::SetupEnvDefault => {
            Some("space toggles".to_owned())
        }
        F::IssuePrRepo if fields.github_issue_pr_repo.trim().is_empty() => {
            Some("blank uses GitHub Repo".to_owned())
        }
        F::IssuePrRepo => Some("override issue/PR tracker".to_owned()),
        F::TransientAgentDir if fields.transient_agent_dir.trim().is_empty() => {
            Some("blank uses /tmp".to_owned())
        }
        F::TransientAgentDir => Some("transient agent work dirs root".to_owned()),
        F::TransientMaxConcurrent
            if fields.transient_max_concurrent.trim().is_empty()
                || fields.transient_max_concurrent.trim() == "0" =>
        {
            Some("0 = no limit".to_owned())
        }
        F::TransientMaxConcurrent => Some("max concurrent transient agents".to_owned()),
        _ => None,
    }
}

fn field_row(label: &str, value: &str, hint: Option<&str>, width: usize) -> String {
    let prefix = format!("  {label:<LABEL_WIDTH$} [");
    let suffix = hint.map_or_else(|| "]".to_owned(), |hint| format!("]  ({hint})"));
    let fixed_width = UnicodeWidthStr::width(prefix.as_str())
        .saturating_add(UnicodeWidthStr::width(suffix.as_str()));
    let Some(value_width) = width.checked_sub(fixed_width) else {
        return fit_text_to_width(&format!("{prefix}{value}{suffix}"), width);
    };
    format!("{prefix}{}{suffix}", fit_text_to_width(value, value_width))
}
fn row_style(
    slot: RepositoryFormFocus,
    focused: bool,
    remote_enabled: bool,
) -> HostControlRowStyle {
    use RepositoryFormFocus as F;
    let remote_setting = matches!(
        slot,
        F::LoginUser | F::Host | F::SshPort | F::IdentityFile | F::SshOptions | F::RunAsUser
    );
    if remote_setting && !remote_enabled {
        HostControlRowStyle::Dim
    } else if focused {
        HostControlRowStyle::Bright
    } else {
        HostControlRowStyle::Normal
    }
}

struct RepositoryFormParts {
    rows: Vec<HostControlRow>,
    declared: Vec<Field>,
    values: TypedMap,
    focus_target: Option<Id>,
}

fn build_repository_form_parts(
    state: &AppState,
    fields: &RepositoryFormFields,
    focus: RepositoryFormFocus,
    cursor: &RepositoryFormCursor,
    width: usize,
) -> RepositoryFormParts {
    let type_id = crate::state::type_id_from_form_value(&fields.default_type_id);
    let mut parts = RepositoryFormParts {
        rows: vec![HostControlRow::plain(String::new())],
        declared: Vec::new(),
        values: TypedMap::new(),
        focus_target: None,
    };

    for (field, slot, gated) in REPOSITORY_FORM_LAYOUT {
        if gated && !crate::state::is_repository_field_visible(slot, type_id.as_ref()) {
            continue;
        }

        let declaration = Field::internal(field);
        let field_id = declaration.id().clone();
        let focused = slot == focus;
        let value = display_value(fields, cursor, slot, focused);
        let hint = field_hint(state, fields, slot);
        parts.rows.push(
            HostControlRow::targeted(
                field_row(
                    display_label(field, declaration.label()),
                    &value,
                    hint.as_deref(),
                    width,
                ),
                PanelHitTarget::Field(field_id.clone()),
            )
            .with_style(row_style(slot, focused, fields.remote_enabled)),
        );
        parts
            .values
            .insert(field_id.clone(), repository_field_value(fields, slot));
        if focused {
            parts.focus_target = Some(field_id);
        }
        parts.declared.push(declaration);
    }

    parts
}

fn finish_repository_form(
    title: &str,
    mut parts: RepositoryFormParts,
    error: Option<&str>,
    width: usize,
) -> OverlayControlProjection {
    parts.rows.push(HostControlRow::targeted(
        String::new(),
        PanelHitTarget::Submit,
    ));
    if let Some(error) = error {
        parts.rows.push(
            HostControlRow::plain(fit_text_to_width(&format!("  Error: {error}"), width))
                .with_style(HostControlRowStyle::Bright),
        );
    }

    let affordances = vec![Affordance {
        id: Id::internal(InternalId::OverlaySubmit),
        label: "Apply".to_owned(),
        action_id: ActionId::internal(InternalActionId::OverlaySubmit),
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    }];
    bespoke_form_projection(
        title,
        parts.rows,
        parts.declared,
        parts.values,
        affordances,
        parts.focus_target,
    )
    .with_title_style(HostControlTitleStyle::Plain)
}

/// Project the open repository form (new or edit) as a shared-shell form
/// control. Visibility follows the old renderer: identity and connection fields
/// always show, while type-gated defaults follow the selected agent type.
#[must_use]
pub fn project_repository_form(state: &AppState, width: usize) -> Option<OverlayControlProjection> {
    let (title, fields, focus, cursor) = match &state.modal {
        ModalState::NewRepository {
            fields,
            focus,
            cursor,
        } => (" New Repository", fields, focus, cursor),
        ModalState::EditRepository {
            fields,
            focus,
            cursor,
            ..
        } => (" Edit Repository", fields, focus, cursor),
        _ => return None,
    };
    let parts = build_repository_form_parts(state, fields, *focus, cursor, width);
    Some(finish_repository_form(
        title,
        parts,
        state.error_message.as_deref(),
        width,
    ))
}
