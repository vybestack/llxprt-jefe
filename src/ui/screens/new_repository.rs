//! New repository form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 34-37

use iocraft::prelude::*;

use crate::selection::SelectablePane;
use crate::state::{AppState, ModalState, RepositoryFormCursor, RepositoryFormFocus};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;
use crate::ui::util::text_with_caret;

/// Props for the new repository form.
#[derive(Default, Props)]
pub struct NewRepositoryFormProps {
    /// Application state (cloned).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Form for creating/editing a repository.
#[component]
pub fn NewRepositoryForm(props: &NewRepositoryFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let sel = SelectionColors::from_resolved(&rc);

    // Extract form state from modal
    let (title, fields, focus, cursor) = props.state.as_ref().map_or_else(
        || {
            (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
                RepositoryFormCursor::default(),
            )
        },
        |state| match &state.modal {
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            } => ("New Repository", fields.clone(), *focus, cursor.clone()),
            ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => ("Edit Repository", fields.clone(), *focus, cursor.clone()),
            _ => (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
                RepositoryFormCursor::default(),
            ),
        },
    );
    let selection = props.state.as_ref().and_then(|s| s.selection);
    let pane = SelectablePane::RepositoryForm;
    let mut line_idx: usize = 0;

    // Content line 0: title, line 1: blank.
    let mut all_lines: Vec<AnyElement<'static>> = Vec::new();
    all_lines.push(selectable_line(
        &format!(" {title}"),
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));
    all_lines.push(selectable_line(
        "",
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));

    // Initial text fields precede the runtime selector.
    let labels = ["Name", "Base Dir", "Default Profile"];
    let values = [&fields.name, &fields.base_dir, &fields.default_profile];
    let focuses = [
        RepositoryFormFocus::Name,
        RepositoryFormFocus::BaseDir,
        RepositoryFormFocus::DefaultProfile,
    ];
    let cursors = [cursor.name, cursor.base_dir, cursor.default_profile];
    for (((label, value), field_focus), field_cursor) in labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
    {
        let is_focused = focus == *field_focus;
        let rendered_value = if is_focused {
            text_with_caret(value, *field_cursor)
        } else {
            (*value).to_owned()
        };
        let display = format!("  {label:<16} [{rendered_value}]");
        let color = if is_focused { rc.bright } else { rc.fg };
        all_lines.push(selectable_line(
            &display,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            color,
            sel,
        ));
    }

    let default_kind = crate::state::kind_from_form_value(&fields.default_agent_kind);
    if crate::state::is_repository_field_visible(
        RepositoryFormFocus::DefaultCodePuppyModel,
        default_kind,
    ) {
        let model_focused = focus == RepositoryFormFocus::DefaultCodePuppyModel;
        let model_value = if model_focused {
            text_with_caret(
                &fields.default_code_puppy_model,
                cursor.default_code_puppy_model,
            )
        } else {
            fields.default_code_puppy_model.clone()
        };
        let model_line = format!("  {:<16} [{model_value}]", "Default Model");
        all_lines.push(selectable_line(
            &model_line,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if model_focused { rc.bright } else { rc.fg },
            sel,
        ));

        // Default Code Puppy YOLO for transient agents (issue #213).
        let yolo_focused = focus == RepositoryFormFocus::DefaultCodePuppyYolo;
        let yolo_mark = if fields.default_code_puppy_yolo {
            "x"
        } else {
            " "
        };
        let yolo_line = format!("  {:<16} [{}]  (space toggles)", "Default YOLO", yolo_mark);
        all_lines.push(selectable_line(
            &yolo_line,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if yolo_focused { rc.bright } else { rc.fg },
            sel,
        ));
    }

    let kind_focused = focus == RepositoryFormFocus::DefaultAgentKind;
    let kind_color = if kind_focused { rc.bright } else { rc.fg };
    let effective_kinds = crate::state::effective_agent_kinds(
        props
            .state
            .as_ref()
            .map_or(&[][..], |s| s.installed_agent_kinds.as_slice()),
        fields.remote_enabled,
    );
    let kind_hint = crate::state::effective_kinds_hint(&effective_kinds);
    let kind_line = format!(
        "  {:<16} [{}]  ({kind_hint})",
        "Default Agent", fields.default_agent_kind
    );
    all_lines.push(selectable_line(
        &kind_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        kind_color,
        sel,
    ));

    if crate::state::is_repository_field_visible(
        RepositoryFormFocus::DefaultLlxprtVersion,
        default_kind,
    ) {
        let version_focused = focus == RepositoryFormFocus::DefaultLlxprtVersion;
        let version_value = if version_focused {
            text_with_caret(
                &fields.default_llxprt_version,
                cursor.default_llxprt_version,
            )
        } else {
            fields.default_llxprt_version.clone()
        };
        let version_line = format!("  {:<16} [{version_value}]", "Default Version");
        all_lines.push(selectable_line(
            &version_line,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if version_focused { rc.bright } else { rc.fg },
            sel,
        ));
    }

    let github_focused = focus == RepositoryFormFocus::GitHubRepo;
    let github_value = if github_focused {
        text_with_caret(&fields.github_repo, cursor.github_repo)
    } else {
        fields.github_repo.clone()
    };
    let github_line = format!("  {:<16} [{github_value}]", "GitHub Repo");
    all_lines.push(selectable_line(
        &github_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if github_focused { rc.bright } else { rc.fg },
        sel,
    ));

    let issue_pr_focused = focus == RepositoryFormFocus::IssuePrRepo;
    let issue_pr_value = if issue_pr_focused {
        text_with_caret(&fields.github_issue_pr_repo, cursor.github_issue_pr_repo)
    } else {
        fields.github_issue_pr_repo.clone()
    };
    let issue_pr_hint = if fields.github_issue_pr_repo.trim().is_empty() {
        "blank uses GitHub Repo"
    } else {
        "override issue/PR tracker"
    };
    let issue_pr_line = format!(
        "  {:<16} [{issue_pr_value}]  ({issue_pr_hint})",
        "Issues / PRs Repo"
    );
    all_lines.push(selectable_line(
        &issue_pr_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if issue_pr_focused { rc.bright } else { rc.fg },
        sel,
    ));

    // Content line 6: Remote Repository checkbox.
    let remote_focused = focus == RepositoryFormFocus::RemoteEnabled;
    let remote_mark = if fields.remote_enabled { "x" } else { " " };
    let remote_color = if remote_focused { rc.bright } else { rc.fg };
    let remote_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Remote Repository", remote_mark
    );
    all_lines.push(selectable_line(
        &remote_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        remote_color,
        sel,
    ));

    // Content lines 7-9: remote fields.
    let remote_labels = [
        "Login User",
        "Host / IP",
        "SSH Port",
        "Identity File",
        "SSH Options (space-separated)",
        "Run As User",
    ];
    let remote_values = [
        &fields.login_user,
        &fields.host,
        &fields.ssh_port,
        &fields.identity_file,
        &fields.ssh_options,
        &fields.run_as_user,
    ];
    let remote_focuses = [
        RepositoryFormFocus::LoginUser,
        RepositoryFormFocus::Host,
        RepositoryFormFocus::SshPort,
        RepositoryFormFocus::IdentityFile,
        RepositoryFormFocus::SshOptions,
        RepositoryFormFocus::RunAsUser,
    ];
    let remote_cursors = [
        cursor.login_user,
        cursor.host,
        cursor.ssh_port,
        cursor.identity_file,
        cursor.ssh_options,
        cursor.run_as_user,
    ];
    for (((label, value), field_focus), field_cursor) in remote_labels
        .iter()
        .zip(remote_values.iter())
        .zip(remote_focuses.iter())
        .zip(remote_cursors.iter())
    {
        let is_focused = focus == *field_focus;
        let rendered_value = if is_focused {
            text_with_caret(value, *field_cursor)
        } else {
            (*value).to_owned()
        };
        let color = if fields.remote_enabled {
            if is_focused { rc.bright } else { rc.fg }
        } else {
            rc.dim
        };
        let display = format!("  {label:<16} [{rendered_value}]");
        all_lines.push(selectable_line(
            &display,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            color,
            sel,
        ));
    }

    // Content line 10: Setup Env Default checkbox.
    let setup_focused = focus == RepositoryFormFocus::SetupEnvDefault;
    let setup_mark = if fields.setup_env_default { "x" } else { " " };
    let setup_color = if fields.remote_enabled {
        if setup_focused { rc.bright } else { rc.fg }
    } else {
        rc.dim
    };
    let setup_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Setup Env Default", setup_mark
    );
    all_lines.push(selectable_line(
        &setup_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        setup_color,
        sel,
    ));

    // Transient agent directory (issue #213).
    let transient_dir_focused = focus == RepositoryFormFocus::TransientAgentDir;
    let transient_dir_value = if transient_dir_focused {
        text_with_caret(&fields.transient_agent_dir, cursor.transient_agent_dir)
    } else {
        fields.transient_agent_dir.clone()
    };
    let transient_dir_hint = if fields.transient_agent_dir.trim().is_empty() {
        "blank uses /tmp"
    } else {
        "transient agent work dirs root"
    };
    let transient_dir_line = format!(
        "  {:<16} [{transient_dir_value}]  ({transient_dir_hint})",
        "Transient Dir"
    );
    all_lines.push(selectable_line(
        &transient_dir_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if transient_dir_focused {
            rc.bright
        } else {
            rc.fg
        },
        sel,
    ));

    // Max concurrent transient agents (issue #213).
    let max_conc_focused = focus == RepositoryFormFocus::TransientMaxConcurrent;
    let max_conc_value = if max_conc_focused {
        text_with_caret(
            &fields.transient_max_concurrent,
            cursor.transient_max_concurrent,
        )
    } else {
        fields.transient_max_concurrent.clone()
    };
    let max_conc_hint = if fields.transient_max_concurrent.trim().is_empty()
        || fields.transient_max_concurrent.trim() == "0"
    {
        "0 = no limit"
    } else {
        "max concurrent transient agents"
    };
    let max_conc_line = format!(
        "  {:<16} [{max_conc_value}]  ({max_conc_hint})",
        "Max Transient"
    );
    all_lines.push(selectable_line(
        &max_conc_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if max_conc_focused { rc.bright } else { rc.fg },
        sel,
    ));

    // Content line 11: blank, line 12: hints.
    all_lines.push(selectable_line(
        "",
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));
    if let Some(error) = props
        .state
        .as_ref()
        .and_then(|state| state.error_message.as_deref())
    {
        all_lines.push(selectable_line(
            &format!("  Error: {error}"),
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.bright,
            sel,
        ));
    }
    all_lines.push(selectable_line(
        "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles remote options  Enter submit  Esc cancel",
        line_idx,
        selection,
        pane,
        rc.dim,
        sel,
    ));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            Box(
                border_style: BorderStyle::Round,
                border_color: rc.border_focused,
                background_color: rc.bg,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                padding: 1i32,
            ) {
                #(all_lines)
            }
        }
    }
}
