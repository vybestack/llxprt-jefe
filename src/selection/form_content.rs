//! Pure content projections for the agent-definition and repository-definition
//! forms (issue #178).
//!
//! These build the flat list of copyable lines that match what the
//! `NewAgentForm` and `NewRepositoryForm` iocraft components render, so mouse
//! selection coordinates map to the exact characters on screen — including the
//! `▏` caret inserted into the focused field's value.
//!
//! All functions are pure and `#[must_use]`. They read only from
//! [`crate::state::AppState`] (no iocraft, no side effects).

use crate::domain::PlatformCapabilities;
use crate::state::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, AppState, ModalState, RepositoryFormCursor,
    RepositoryFormFields, RepositoryFormFocus, agent_form_visibility, effective_agent_kinds,
    effective_kinds_hint, is_field_visible, is_repository_field_visible, kind_from_form_value,
};
use crate::ui::util::text_with_caret;

const AGENT_HINT: &str = "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles/cycles checkboxes  Enter submit  Esc cancel";
const REPO_HINT: &str = "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles remote options  Enter submit  Esc cancel";

/// Render a single form field row: `  {label:<16} [{value}]`.
fn render_field(label: &str, value: &str) -> String {
    format!("  {label:<16} [{value}]")
}

/// Render a checkbox row: `  {label:<16} [{mark}]  ({hint})`.
fn render_checkbox(label: &str, checked: bool, hint: &str) -> String {
    let mark = if checked { "x" } else { " " };
    format!("  {label:<16} [{mark}]  ({hint})")
}

/// Build the field specs for the agent form's editable text fields rendered
/// BEFORE the Agent Runtime selector (Shortcut, Name, Description, Work Dir,
/// Profile).
fn agent_pre_kind_field_specs(
    fields: &AgentFormFields,
    cursor: &AgentFormCursor,
) -> [(&'static str, String, AgentFormFocus, usize); 5] {
    let shortcut = fields
        .shortcut_slot
        .map_or_else(|| "none".to_string(), |slot| slot.to_string());
    [
        ("Shortcut (1-9)", shortcut, AgentFormFocus::Shortcut, 0),
        (
            "Name",
            fields.name.clone(),
            AgentFormFocus::Name,
            cursor.name,
        ),
        (
            "Description",
            fields.description.clone(),
            AgentFormFocus::Description,
            cursor.description,
        ),
        (
            "Work Dir",
            fields.work_dir.clone(),
            AgentFormFocus::WorkDir,
            cursor.work_dir,
        ),
        (
            "Profile",
            fields.profile.clone(),
            AgentFormFocus::Profile,
            cursor.profile,
        ),
    ]
}

/// Build the field specs for the agent form's editable text fields rendered
/// AFTER the Agent Runtime selector (Mode Flags, LLXPRT_DEBUG).
fn agent_post_kind_field_specs(
    fields: &AgentFormFields,
    cursor: &AgentFormCursor,
) -> Vec<(&'static str, String, AgentFormFocus, usize)> {
    vec![
        (
            "Model",
            fields.code_puppy_model.clone(),
            AgentFormFocus::CodePuppyModel,
            cursor.code_puppy_model,
        ),
        (
            "Version",
            fields.code_puppy_version.clone(),
            AgentFormFocus::CodePuppyVersion,
            cursor.code_puppy_version,
        ),
        (
            "Mode Flags",
            fields.mode.clone(),
            AgentFormFocus::Mode,
            cursor.mode,
        ),
        (
            "Version",
            fields.llxprt_version.clone(),
            AgentFormFocus::LlxprtVersion,
            cursor.llxprt_version,
        ),
        (
            "LLXPRT_DEBUG",
            fields.llxprt_debug.clone(),
            AgentFormFocus::LlxprtDebug,
            cursor.llxprt_debug,
        ),
    ]
}

/// Build the copyable content lines for the agent-definition form.
#[must_use]
pub fn agent_form_content_lines(state: &AppState) -> Option<Vec<String>> {
    let (title, fields, focus, cursor) = agent_form_state(state)?;
    let mut lines: Vec<String> = vec![format!(" {title}"), String::new()];

    let visibility = agent_form_visibility(kind_from_form_value(&fields.agent_kind));

    // Render pre-kind text fields, then Agent Runtime, then post-kind fields
    // so the line order matches the focus/navigation order
    // (WorkDir → Profile → AgentKind → Mode).
    let pre_specs = agent_pre_kind_field_specs(fields, cursor);
    agent_text_field_lines(&pre_specs, focus, visibility, &mut lines);

    let effective_kinds = effective_kinds_for_agent_form(state);
    let kind_hint = effective_kinds_hint(&effective_kinds);
    lines.push(format!(
        "  {:<16} [{}]  ({kind_hint})",
        "Agent Runtime", fields.agent_kind
    ));

    let post_specs = agent_post_kind_field_specs(fields, cursor);
    agent_text_field_lines(&post_specs, focus, visibility, &mut lines);

    if visibility.shows_llxprt_fields() {
        lines.push(render_checkbox(
            "Pass --continue",
            fields.pass_continue,
            "space toggles",
        ));
        lines.push(render_checkbox(
            "Sandbox",
            fields.sandbox_enabled,
            "space toggles",
        ));

        let engine_hint = sandbox_engine_hint(fields.sandbox_enabled);
        lines.push(format!(
            "  {:<16} [{}]  ({engine_hint})",
            "Sandbox Engine", fields.sandbox_engine
        ));

        let flags_value = if focus == AgentFormFocus::SandboxFlags {
            text_with_caret(&fields.sandbox_flags, cursor.sandbox_flags)
        } else {
            fields.sandbox_flags.clone()
        };
        lines.push(render_field("Sandbox Flags", &flags_value));
    } else {
        lines.push(render_checkbox(
            "YOLO",
            fields.code_puppy_yolo,
            "space toggles",
        ));
        lines.push(render_checkbox(
            "Quick resume",
            fields.code_puppy_quick_resume.enabled(),
            "space toggles",
        ));
    }

    lines.push(String::new());
    lines.push(AGENT_HINT.to_string());
    Some(lines)
}

/// Append rendered text-field rows, inserting the caret into the focused
/// editable field (Shortcut is never editable). Hidden fields are skipped so
/// the line indices match the on-screen render.
fn agent_text_field_lines(
    specs: &[(&str, String, AgentFormFocus, usize)],
    focus: AgentFormFocus,
    visibility: crate::state::AgentFormFieldVisibility,
    lines: &mut Vec<String>,
) {
    for (label, value, field_focus, field_cursor) in specs {
        if !is_field_visible(*field_focus, visibility) {
            continue;
        }
        let rendered = if focus == *field_focus && *field_focus != AgentFormFocus::Shortcut {
            text_with_caret(value, *field_cursor)
        } else {
            value.clone()
        };
        lines.push(render_field(label, &rendered));
    }
}

/// Extract the agent form title, fields, focus, and cursor from the active modal state.
fn agent_form_state(
    state: &AppState,
) -> Option<(&str, &AgentFormFields, AgentFormFocus, &AgentFormCursor)> {
    match &state.modal {
        ModalState::NewAgent {
            fields,
            focus,
            cursor,
            ..
        } => Some(("New Agent", fields, *focus, cursor)),
        ModalState::EditAgent {
            fields,
            focus,
            cursor,
            ..
        } => Some(("Edit Agent", fields, *focus, cursor)),
        _ => None,
    }
}

/// Build the field specs for the repository form's text fields.
fn repo_text_field_specs(
    fields: &RepositoryFormFields,
    cursor: &RepositoryFormCursor,
) -> [(&'static str, String, RepositoryFormFocus, usize); 3] {
    [
        (
            "Name",
            fields.name.clone(),
            RepositoryFormFocus::Name,
            cursor.name,
        ),
        (
            "Base Dir",
            fields.base_dir.clone(),
            RepositoryFormFocus::BaseDir,
            cursor.base_dir,
        ),
        (
            "Default Profile",
            fields.default_profile.clone(),
            RepositoryFormFocus::DefaultProfile,
            cursor.default_profile,
        ),
    ]
}

/// Build the field specs for the repository form's remote fields.
fn repo_remote_field_specs(
    fields: &RepositoryFormFields,
    cursor: &RepositoryFormCursor,
) -> [(&'static str, String, RepositoryFormFocus, usize); 3] {
    [
        (
            "Login User",
            fields.login_user.clone(),
            RepositoryFormFocus::LoginUser,
            cursor.login_user,
        ),
        (
            "Host / IP",
            fields.host.clone(),
            RepositoryFormFocus::Host,
            cursor.host,
        ),
        (
            "Run As User",
            fields.run_as_user.clone(),
            RepositoryFormFocus::RunAsUser,
            cursor.run_as_user,
        ),
    ]
}

fn append_repository_runtime_fields(
    state: &AppState,
    fields: &RepositoryFormFields,
    focus: RepositoryFormFocus,
    cursor: &RepositoryFormCursor,
    lines: &mut Vec<String>,
) {
    let default_kind = kind_from_form_value(&fields.default_agent_kind);
    if is_repository_field_visible(RepositoryFormFocus::DefaultCodePuppyModel, default_kind) {
        let value = focused_value(
            &fields.default_code_puppy_model,
            cursor.default_code_puppy_model,
            focus == RepositoryFormFocus::DefaultCodePuppyModel,
        );
        lines.push(render_field("Default Model", &value));
    }
    let kinds = effective_agent_kinds(&state.installed_agent_kinds, fields.remote_enabled);
    lines.push(format!(
        "  {:<16} [{}]  ({})",
        "Default Agent",
        fields.default_agent_kind,
        effective_kinds_hint(&kinds)
    ));
    if is_repository_field_visible(RepositoryFormFocus::DefaultCodePuppyYolo, default_kind) {
        lines.push(render_checkbox(
            "Default YOLO",
            fields.default_code_puppy_yolo,
            "space toggles",
        ));
    }
    if is_repository_field_visible(RepositoryFormFocus::DefaultCodePuppyVersion, default_kind) {
        let value = focused_value(
            &fields.default_code_puppy_version,
            cursor.default_code_puppy_version,
            focus == RepositoryFormFocus::DefaultCodePuppyVersion,
        );
        lines.push(render_field("Default Version", &value));
    }
    if is_repository_field_visible(RepositoryFormFocus::DefaultLlxprtMode, default_kind) {
        let value = focused_value(
            &fields.default_llxprt_mode,
            cursor.default_llxprt_mode,
            focus == RepositoryFormFocus::DefaultLlxprtMode,
        );
        lines.push(render_field("Default Mode", &value));
    }
    if is_repository_field_visible(RepositoryFormFocus::DefaultLlxprtVersion, default_kind) {
        let value = focused_value(
            &fields.default_llxprt_version,
            cursor.default_llxprt_version,
            focus == RepositoryFormFocus::DefaultLlxprtVersion,
        );
        lines.push(render_field("Default Version", &value));
    }
}

fn focused_value(value: &str, cursor: usize, focused: bool) -> String {
    if focused {
        text_with_caret(value, cursor)
    } else {
        value.to_owned()
    }
}

/// Build the copyable content lines for the repository-definition form.
#[must_use]
pub fn repository_form_content_lines(state: &AppState) -> Option<Vec<String>> {
    let (title, fields, focus, cursor) = repository_form_state(state)?;
    let mut lines = vec![format!(" {title}"), String::new()];

    let text_specs = repo_text_field_specs(fields, cursor);
    repo_text_field_lines(&text_specs, focus, &mut lines);

    append_repository_runtime_fields(state, fields, focus, cursor, &mut lines);

    let github_value = if focus == RepositoryFormFocus::GitHubRepo {
        text_with_caret(&fields.github_repo, cursor.github_repo)
    } else {
        fields.github_repo.clone()
    };
    lines.push(render_field("GitHub Repo", &github_value));

    let issue_pr_value = if focus == RepositoryFormFocus::IssuePrRepo {
        text_with_caret(&fields.github_issue_pr_repo, cursor.github_issue_pr_repo)
    } else {
        fields.github_issue_pr_repo.clone()
    };
    let issue_pr_hint = if fields.github_issue_pr_repo.trim().is_empty() {
        "blank uses GitHub Repo"
    } else {
        "override issue/PR tracker"
    };
    lines.push(format!(
        "  {:<16} [{issue_pr_value}]  ({issue_pr_hint})",
        "Issues / PRs Repo"
    ));

    lines.push(render_checkbox(
        "Remote Repository",
        fields.remote_enabled,
        "space toggles",
    ));

    let remote_specs = repo_remote_field_specs(fields, cursor);
    repo_text_field_lines(&remote_specs, focus, &mut lines);

    lines.push(render_checkbox(
        "Setup Env Default",
        fields.setup_env_default,
        "space toggles",
    ));
    if let Some(error) = state.error_message.as_deref() {
        lines.push(format!("  Error: {error}"));
    }
    lines.push(String::new());
    lines.push(REPO_HINT.to_string());
    Some(lines)
}

/// Append rendered text-field rows for the repository form, inserting the
/// caret into the focused field.
fn repo_text_field_lines(
    specs: &[(&str, String, RepositoryFormFocus, usize)],
    focus: RepositoryFormFocus,
    lines: &mut Vec<String>,
) {
    for (label, value, field_focus, field_cursor) in specs {
        let rendered = if focus == *field_focus {
            text_with_caret(value, *field_cursor)
        } else {
            value.clone()
        };
        lines.push(render_field(label, &rendered));
    }
}

/// Extract the repository form title, fields, focus, and cursor from the active modal state.
fn repository_form_state(
    state: &AppState,
) -> Option<(
    &str,
    &RepositoryFormFields,
    RepositoryFormFocus,
    &RepositoryFormCursor,
)> {
    match &state.modal {
        ModalState::NewRepository {
            fields,
            focus,
            cursor,
            ..
        } => Some(("New Repository", fields, *focus, cursor)),
        ModalState::EditRepository {
            fields,
            focus,
            cursor,
            ..
        } => Some(("Edit Repository", fields, *focus, cursor)),
        _ => None,
    }
}

/// Compute the sandbox-engine hint shown on the form, matching the renderer.
fn sandbox_engine_hint(sandbox_enabled: bool) -> String {
    if sandbox_enabled {
        let labels: Vec<&str> = PlatformCapabilities::current()
            .supported_engines()
            .iter()
            .map(|e| e.label())
            .collect();
        format!("space cycles: {}", labels.join(" / "))
    } else {
        String::from("disabled")
    }
}

/// Resolve the effective agent kinds for the currently open agent form,
/// matching what the form-state cycling logic offers. Remote-enabled
/// repositories offer both kinds; local repositories offer only installed
/// kinds.
fn effective_kinds_for_agent_form(state: &AppState) -> Vec<crate::domain::AgentKind> {
    let is_remote = match &state.modal {
        ModalState::NewAgent { repository_id, .. } => state
            .repository_by_id(repository_id)
            .is_some_and(|r| r.remote.enabled),
        ModalState::EditAgent { id, .. } => state
            .repository_for_agent(id)
            .is_some_and(|r| r.remote.enabled),
        _ => false,
    };
    effective_agent_kinds(&state.installed_agent_kinds, is_remote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RepositoryId;
    use crate::state::{AgentFormCursor, AgentFormFields, AgentFormFocus, ModalState};

    #[test]
    fn agent_form_title_has_leading_space() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields::default(),
                focus: AgentFormFocus::Shortcut,
                cursor: AgentFormCursor::default(),
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        assert_eq!(
            lines[0], " New Agent",
            "title must include the leading space"
        );
    }

    #[test]
    fn agent_form_focused_name_field_has_caret() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields {
                    name: "abc".to_string(),
                    ..Default::default()
                },
                focus: AgentFormFocus::Name,
                cursor: AgentFormCursor {
                    name: 1,
                    ..Default::default()
                },
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        let name_line = lines
            .iter()
            .find(|l| l.contains("Name"))
            .unwrap_or_else(|| panic!("must have name line"));
        assert!(
            name_line.contains("a▏bc"),
            "focused name field must have caret at cursor position 1"
        );
    }

    #[test]
    fn agent_form_unfocused_field_has_no_caret() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields {
                    name: "abc".to_string(),
                    ..Default::default()
                },
                focus: AgentFormFocus::Description,
                cursor: AgentFormCursor::default(),
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        let name_line = lines
            .iter()
            .find(|l| l.contains("Name"))
            .unwrap_or_else(|| panic!("must have name line"));
        assert!(
            !name_line.contains('▏'),
            "unfocused name field must not have caret"
        );
    }

    #[test]
    fn agent_form_sandbox_disabled_shows_disabled_hint() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields {
                    sandbox_enabled: false,
                    ..Default::default()
                },
                focus: AgentFormFocus::Shortcut,
                cursor: AgentFormCursor::default(),
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        let engine_line = lines
            .iter()
            .find(|l| l.contains("Sandbox Engine"))
            .unwrap_or_else(|| panic!("must have engine line"));
        assert!(
            engine_line.contains("(disabled)"),
            "disabled sandbox must show (disabled) hint"
        );
    }

    #[test]
    fn agent_form_sandbox_enabled_shows_cycle_hint() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields {
                    sandbox_enabled: true,
                    ..Default::default()
                },
                focus: AgentFormFocus::Shortcut,
                cursor: AgentFormCursor::default(),
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        let engine_line = lines
            .iter()
            .find(|l| l.contains("Sandbox Engine"))
            .unwrap_or_else(|| panic!("must have engine line"));
        assert!(
            engine_line.contains("space cycles:"),
            "enabled sandbox must show cycle hint, got: {engine_line}"
        );
    }

    #[test]
    fn agent_form_sandbox_flags_focused_has_caret() {
        let state = AppState {
            modal: ModalState::NewAgent {
                repository_id: RepositoryId("r1".to_string()),
                fields: AgentFormFields {
                    sandbox_flags: "--network".to_string(),
                    ..Default::default()
                },
                focus: AgentFormFocus::SandboxFlags,
                cursor: AgentFormCursor {
                    sandbox_flags: 2,
                    ..Default::default()
                },
                work_dir_manual: false,
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        let flags_line = lines
            .iter()
            .find(|l| l.contains("Sandbox Flags"))
            .unwrap_or_else(|| panic!("must have flags line"));
        assert!(
            flags_line.contains("--▏network"),
            "focused flags field must have caret at position 2"
        );
    }

    #[test]
    fn edit_agent_form_title_has_leading_space() {
        let state = AppState {
            modal: ModalState::EditAgent {
                id: crate::domain::AgentId("a1".to_string()),
                fields: AgentFormFields::default(),
                focus: AgentFormFocus::Shortcut,
                cursor: AgentFormCursor::default(),
            },
            ..Default::default()
        };
        let Some(lines) = agent_form_content_lines(&state) else {
            panic!("must have lines");
        };
        assert_eq!(lines[0], " Edit Agent");
    }
}
