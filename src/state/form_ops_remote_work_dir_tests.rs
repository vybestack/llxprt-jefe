//! State-level regression coverage for remote agent work-directory derivation.

use super::*;
use crate::domain::{RemoteRepositorySettings, Repository, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::types::ModalState;

#[test]
fn remote_agent_work_dir_preserves_unix_tilde_and_trims_trailing_slashes() {
    let repository_id = RepositoryId("remote-repo".to_owned());
    let mut repository = Repository::new(
        repository_id.clone(),
        "Remote Repo".to_owned(),
        "remote-repo".to_owned(),
        std::path::PathBuf::from("~/remote///"),
    );
    repository.remote = RemoteRepositorySettings {
        enabled: true,
        ..RemoteRepositorySettings::default()
    };
    let mut state = AppState {
        repositories: vec![repository],
        ..AppState::default()
    };

    state = state.apply(AppEvent::OpenNewAgent(repository_id));
    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Branch 1".to_owned();
    state.update_agent_work_dir_from_name();

    let ModalState::NewAgent { fields, .. } = &state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(fields.work_dir, "~/remote/branch-1");
}
