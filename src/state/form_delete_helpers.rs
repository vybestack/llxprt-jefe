//! Helper functions for repository form field deletion (backspace/delete-at).
//!
//! Extracted from [`form_ops`] to keep that module under the architecture
//! line limit after the transient-agent fields were added.

use super::types::{RepositoryFormCursor, RepositoryFormFields, RepositoryFormFocus};
use super::util::{delete_char_at, delete_char_before};

pub fn delete_transient_field_before(
    fields: &mut RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::TransientAgentDir => {
            cursor.transient_agent_dir =
                delete_char_before(&mut fields.transient_agent_dir, cursor.transient_agent_dir);
        }
        RepositoryFormFocus::TransientMaxConcurrent => {
            cursor.transient_max_concurrent = delete_char_before(
                &mut fields.transient_max_concurrent,
                cursor.transient_max_concurrent,
            );
        }
        _ => {}
    }
}

pub fn delete_transient_field_at(
    fields: &mut RepositoryFormFields,
    cursor: &RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::TransientAgentDir => {
            delete_char_at(&mut fields.transient_agent_dir, cursor.transient_agent_dir);
        }
        RepositoryFormFocus::TransientMaxConcurrent => {
            delete_char_at(
                &mut fields.transient_max_concurrent,
                cursor.transient_max_concurrent,
            );
        }
        _ => {}
    }
}

pub fn delete_remote_field_before(
    fields: &mut RepositoryFormFields,
    cursor: &mut RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::LoginUser => {
            cursor.login_user = delete_char_before(&mut fields.login_user, cursor.login_user);
        }
        RepositoryFormFocus::Host => {
            cursor.host = delete_char_before(&mut fields.host, cursor.host);
        }
        RepositoryFormFocus::SshPort => {
            cursor.ssh_port = delete_char_before(&mut fields.ssh_port, cursor.ssh_port);
        }
        RepositoryFormFocus::IdentityFile => {
            cursor.identity_file =
                delete_char_before(&mut fields.identity_file, cursor.identity_file);
        }
        RepositoryFormFocus::SshOptions => {
            cursor.ssh_options = delete_char_before(&mut fields.ssh_options, cursor.ssh_options);
        }
        RepositoryFormFocus::RunAsUser => {
            cursor.run_as_user = delete_char_before(&mut fields.run_as_user, cursor.run_as_user);
        }
        _ => {}
    }
}

pub fn delete_remote_field_at(
    fields: &mut RepositoryFormFields,
    cursor: &RepositoryFormCursor,
    focus: RepositoryFormFocus,
) {
    match focus {
        RepositoryFormFocus::LoginUser => {
            delete_char_at(&mut fields.login_user, cursor.login_user);
        }
        RepositoryFormFocus::Host => {
            delete_char_at(&mut fields.host, cursor.host);
        }
        RepositoryFormFocus::SshPort => {
            delete_char_at(&mut fields.ssh_port, cursor.ssh_port);
        }
        RepositoryFormFocus::IdentityFile => {
            delete_char_at(&mut fields.identity_file, cursor.identity_file);
        }
        RepositoryFormFocus::SshOptions => {
            delete_char_at(&mut fields.ssh_options, cursor.ssh_options);
        }
        RepositoryFormFocus::RunAsUser => {
            delete_char_at(&mut fields.run_as_user, cursor.run_as_user);
        }
        _ => {}
    }
}
