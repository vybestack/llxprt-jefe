use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, RepositoryFormCursor, RepositoryFormFields,
    RepositoryFormFocus, WorkflowDispatchFormCursor, WorkflowDispatchFormFields,
    WorkflowDispatchFormFocus,
};
use super::util::{insert_char_at, move_cursor_left, move_cursor_right};

pub(super) fn handle_repository_field_char(
    fields: &mut RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
    c: char,
) -> bool {
    match focus {
        RepositoryFormFocus::DefaultAgentKind
        | RepositoryFormFocus::RemoteEnabled
        | RepositoryFormFocus::SetupEnvDefault => c == ' ' || c == 'x' || c == 'X',
        text_focus => {
            insert_repository_text_char(fields, cursor, text_focus, c);
            false
        }
    }
}

fn insert_repository_text_char(
    fields: &mut RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
    c: char,
) {
    match focus {
        RepositoryFormFocus::Name => cursor.name = insert_char_at(&mut fields.name, cursor.name, c),
        RepositoryFormFocus::BaseDir => {
            cursor.base_dir = insert_char_at(&mut fields.base_dir, cursor.base_dir, c);
        }
        RepositoryFormFocus::DefaultProfile => {
            cursor.default_profile =
                insert_char_at(&mut fields.default_profile, cursor.default_profile, c);
        }
        RepositoryFormFocus::DefaultCodePuppyModel => {
            cursor.default_code_puppy_model = insert_char_at(
                &mut fields.default_code_puppy_model,
                cursor.default_code_puppy_model,
                c,
            );
        }
        RepositoryFormFocus::DefaultLlxprtVersion => {
            cursor.default_llxprt_version = insert_char_at(
                &mut fields.default_llxprt_version,
                cursor.default_llxprt_version,
                c,
            );
        }
        RepositoryFormFocus::GitHubRepo => {
            cursor.github_repo = insert_char_at(&mut fields.github_repo, cursor.github_repo, c);
        }
        RepositoryFormFocus::IssuePrRepo => {
            cursor.github_issue_pr_repo = insert_char_at(
                &mut fields.github_issue_pr_repo,
                cursor.github_issue_pr_repo,
                c,
            );
        }
        RepositoryFormFocus::LoginUser => {
            cursor.login_user = insert_char_at(&mut fields.login_user, cursor.login_user, c);
        }
        RepositoryFormFocus::Host => cursor.host = insert_char_at(&mut fields.host, cursor.host, c),
        RepositoryFormFocus::SshPort => {
            cursor.ssh_port = insert_char_at(&mut fields.ssh_port, cursor.ssh_port, c);
        }
        RepositoryFormFocus::IdentityFile => {
            cursor.identity_file =
                insert_char_at(&mut fields.identity_file, cursor.identity_file, c);
        }
        RepositoryFormFocus::SshOptions => {
            cursor.ssh_options = insert_char_at(&mut fields.ssh_options, cursor.ssh_options, c);
        }
        RepositoryFormFocus::RunAsUser => {
            cursor.run_as_user = insert_char_at(&mut fields.run_as_user, cursor.run_as_user, c);
        }
        RepositoryFormFocus::DefaultAgentKind
        | RepositoryFormFocus::RemoteEnabled
        | RepositoryFormFocus::SetupEnvDefault => {}
    }
}

pub(super) fn move_repository_field_cursor_right(
    fields: &RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::DefaultAgentKind
        | RepositoryFormFocus::RemoteEnabled
        | RepositoryFormFocus::SetupEnvDefault => {}
        RepositoryFormFocus::Name => cursor.name = move_cursor_right(&fields.name, cursor.name),
        RepositoryFormFocus::BaseDir => {
            cursor.base_dir = move_cursor_right(&fields.base_dir, cursor.base_dir);
        }
        RepositoryFormFocus::DefaultProfile => {
            cursor.default_profile =
                move_cursor_right(&fields.default_profile, cursor.default_profile);
        }
        RepositoryFormFocus::DefaultCodePuppyModel => {
            cursor.default_code_puppy_model = move_cursor_right(
                &fields.default_code_puppy_model,
                cursor.default_code_puppy_model,
            );
        }
        RepositoryFormFocus::DefaultLlxprtVersion => {
            cursor.default_llxprt_version = move_cursor_right(
                &fields.default_llxprt_version,
                cursor.default_llxprt_version,
            );
        }
        RepositoryFormFocus::GitHubRepo => {
            cursor.github_repo = move_cursor_right(&fields.github_repo, cursor.github_repo);
        }
        RepositoryFormFocus::IssuePrRepo => {
            cursor.github_issue_pr_repo =
                move_cursor_right(&fields.github_issue_pr_repo, cursor.github_issue_pr_repo);
        }
        RepositoryFormFocus::LoginUser => {
            cursor.login_user = move_cursor_right(&fields.login_user, cursor.login_user);
        }
        RepositoryFormFocus::Host => cursor.host = move_cursor_right(&fields.host, cursor.host),
        RepositoryFormFocus::SshPort => {
            cursor.ssh_port = move_cursor_right(&fields.ssh_port, cursor.ssh_port);
        }
        RepositoryFormFocus::IdentityFile => {
            cursor.identity_file = move_cursor_right(&fields.identity_file, cursor.identity_file);
        }
        RepositoryFormFocus::SshOptions => {
            cursor.ssh_options = move_cursor_right(&fields.ssh_options, cursor.ssh_options);
        }
        RepositoryFormFocus::RunAsUser => {
            cursor.run_as_user = move_cursor_right(&fields.run_as_user, cursor.run_as_user);
        }
    }
}

pub(super) fn move_agent_field_cursor_right(
    fields: &AgentFormFields,
    cursor: &mut AgentFormCursor,
    focus: AgentFormFocus,
) {
    match focus {
        AgentFormFocus::Shortcut
        | AgentFormFocus::AgentKind
        | AgentFormFocus::CodePuppyYolo
        | AgentFormFocus::CodePuppyQuickResume
        | AgentFormFocus::PassContinue
        | AgentFormFocus::Sandbox
        | AgentFormFocus::SandboxEngine => {}
        AgentFormFocus::Name => cursor.name = move_cursor_right(&fields.name, cursor.name),
        AgentFormFocus::Description => {
            cursor.description = move_cursor_right(&fields.description, cursor.description);
        }
        AgentFormFocus::WorkDir => {
            cursor.work_dir = move_cursor_right(&fields.work_dir, cursor.work_dir);
        }
        AgentFormFocus::Profile => {
            cursor.profile = move_cursor_right(&fields.profile, cursor.profile);
        }
        AgentFormFocus::LlxprtVersion => {
            cursor.llxprt_version =
                move_cursor_right(&fields.llxprt_version, cursor.llxprt_version);
        }
        AgentFormFocus::CodePuppyModel => {
            cursor.code_puppy_model =
                move_cursor_right(&fields.code_puppy_model, cursor.code_puppy_model);
        }
        AgentFormFocus::Mode => cursor.mode = move_cursor_right(&fields.mode, cursor.mode),
        AgentFormFocus::LlxprtDebug => {
            cursor.llxprt_debug = move_cursor_right(&fields.llxprt_debug, cursor.llxprt_debug);
        }
        AgentFormFocus::SandboxFlags => {
            cursor.sandbox_flags = move_cursor_right(&fields.sandbox_flags, cursor.sandbox_flags);
        }
    }
}

pub(super) fn move_workflow_dispatch_field_cursor_right(
    fields: &WorkflowDispatchFormFields,
    cursor: &mut WorkflowDispatchFormCursor,
    focus: WorkflowDispatchFormFocus,
) {
    match focus {
        WorkflowDispatchFormFocus::RefName => {
            cursor.ref_name = move_cursor_right(&fields.ref_name, cursor.ref_name);
        }
        WorkflowDispatchFormFocus::Inputs => {
            cursor.inputs = move_cursor_right(&fields.inputs, cursor.inputs);
        }
        WorkflowDispatchFormFocus::Submit | WorkflowDispatchFormFocus::Cancel => {}
    }
}

pub(super) fn move_repository_field_cursor_left(
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::DefaultAgentKind
        | RepositoryFormFocus::RemoteEnabled
        | RepositoryFormFocus::SetupEnvDefault => {}
        RepositoryFormFocus::Name => cursor.name = move_cursor_left(cursor.name),
        RepositoryFormFocus::BaseDir => cursor.base_dir = move_cursor_left(cursor.base_dir),
        RepositoryFormFocus::DefaultProfile => {
            cursor.default_profile = move_cursor_left(cursor.default_profile);
        }
        RepositoryFormFocus::DefaultCodePuppyModel => {
            cursor.default_code_puppy_model = move_cursor_left(cursor.default_code_puppy_model);
        }
        RepositoryFormFocus::DefaultLlxprtVersion => {
            cursor.default_llxprt_version = move_cursor_left(cursor.default_llxprt_version);
        }
        RepositoryFormFocus::GitHubRepo => {
            cursor.github_repo = move_cursor_left(cursor.github_repo);
        }
        RepositoryFormFocus::IssuePrRepo => {
            cursor.github_issue_pr_repo = move_cursor_left(cursor.github_issue_pr_repo);
        }
        RepositoryFormFocus::LoginUser => cursor.login_user = move_cursor_left(cursor.login_user),
        RepositoryFormFocus::Host => cursor.host = move_cursor_left(cursor.host),
        RepositoryFormFocus::SshPort => cursor.ssh_port = move_cursor_left(cursor.ssh_port),
        RepositoryFormFocus::IdentityFile => {
            cursor.identity_file = move_cursor_left(cursor.identity_file);
        }
        RepositoryFormFocus::SshOptions => {
            cursor.ssh_options = move_cursor_left(cursor.ssh_options);
        }
        RepositoryFormFocus::RunAsUser => cursor.run_as_user = move_cursor_left(cursor.run_as_user),
    }
}

pub(super) fn move_agent_field_cursor_left(cursor: &mut AgentFormCursor, focus: AgentFormFocus) {
    match focus {
        AgentFormFocus::Shortcut
        | AgentFormFocus::AgentKind
        | AgentFormFocus::CodePuppyYolo
        | AgentFormFocus::CodePuppyQuickResume
        | AgentFormFocus::PassContinue
        | AgentFormFocus::Sandbox
        | AgentFormFocus::SandboxEngine => {}
        AgentFormFocus::Name => cursor.name = move_cursor_left(cursor.name),
        AgentFormFocus::Description => cursor.description = move_cursor_left(cursor.description),
        AgentFormFocus::WorkDir => cursor.work_dir = move_cursor_left(cursor.work_dir),
        AgentFormFocus::Profile => cursor.profile = move_cursor_left(cursor.profile),
        AgentFormFocus::LlxprtVersion => {
            cursor.llxprt_version = move_cursor_left(cursor.llxprt_version);
        }
        AgentFormFocus::CodePuppyModel => {
            cursor.code_puppy_model = move_cursor_left(cursor.code_puppy_model);
        }
        AgentFormFocus::Mode => cursor.mode = move_cursor_left(cursor.mode),
        AgentFormFocus::LlxprtDebug => cursor.llxprt_debug = move_cursor_left(cursor.llxprt_debug),
        AgentFormFocus::SandboxFlags => {
            cursor.sandbox_flags = move_cursor_left(cursor.sandbox_flags);
        }
    }
}
