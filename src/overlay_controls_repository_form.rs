//! Repository form (new/edit) projection onto the shared overlay Form control.
//!
//! Mirrors the legacy bespoke renderer: declared field order, type-gated
//! visibility, the focused-field caret rewrite, and the pending error line all
//! ride [`OverlayControlProjection`] so the form draws and takes input through
//! the one shared overlay runtime.

use crate::domain::TypedValue;
use crate::domain::plugin::field::{Field, InternalField};
use crate::host_controls::PanelHitTarget;
use crate::overlay_controls::{OverlayControlProjection, prepend_detail_rows, project_form};
use crate::runtime::provider::protocol::TypedMap;
use crate::state::{
    AppState, ModalState, RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus,
};

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
        // `push_wrapped` may have produced multiple rows carrying the same
        // `Field(id)` target. Only the first row is the focused line;
        // continuation rows must keep their wrapped text so the row count
        // matches the unfocused rendering (issue #706).
        let mut rewritten = false;
        for row in &mut projection.rows {
            if !rewritten && row.target.as_ref() == Some(&PanelHitTarget::Field(id.clone())) {
                row.text =
                    crate::ui::util::truncate_with_ellipsis(&format!("{label}: {caret}"), width);
                rewritten = true;
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
