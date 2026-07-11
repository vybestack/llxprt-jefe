use super::types::{
    AgentFormCursor, AgentFormFields, AgentFormFocus, RepositoryFormCursor, RepositoryFormFields,
    RepositoryFormFocus,
};
use super::util::{insert_char_at, move_cursor_right};

pub(super) fn handle_repository_field_char(
    fields: &mut RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
    c: char,
) -> bool {
    match focus {
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
        RepositoryFormFocus::GitHubRepo => {
            cursor.github_repo = insert_char_at(&mut fields.github_repo, cursor.github_repo, c);
        }
        RepositoryFormFocus::LoginUser => {
            cursor.login_user = insert_char_at(&mut fields.login_user, cursor.login_user, c);
        }
        RepositoryFormFocus::Host => {
            cursor.host = insert_char_at(&mut fields.host, cursor.host, c);
        }
        RepositoryFormFocus::RunAsUser => {
            cursor.run_as_user = insert_char_at(&mut fields.run_as_user, cursor.run_as_user, c);
        }
        RepositoryFormFocus::DefaultAgentKind
        | RepositoryFormFocus::RemoteEnabled
        | RepositoryFormFocus::SetupEnvDefault => {
            return c == ' ' || c == 'x' || c == 'X';
        }
    }
    false
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
        RepositoryFormFocus::GitHubRepo => {
            cursor.github_repo = move_cursor_right(&fields.github_repo, cursor.github_repo);
        }
        RepositoryFormFocus::LoginUser => {
            cursor.login_user = move_cursor_right(&fields.login_user, cursor.login_user);
        }
        RepositoryFormFocus::Host => cursor.host = move_cursor_right(&fields.host, cursor.host),
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
        AgentFormFocus::Mode => cursor.mode = move_cursor_right(&fields.mode, cursor.mode),
        AgentFormFocus::LlxprtDebug => {
            cursor.llxprt_debug = move_cursor_right(&fields.llxprt_debug, cursor.llxprt_debug);
        }
        AgentFormFocus::SandboxFlags => {
            cursor.sandbox_flags = move_cursor_right(&fields.sandbox_flags, cursor.sandbox_flags);
        }
    }
}
