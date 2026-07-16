//! Launch-signature tests for repository transient options (issue #317).

use super::{launch_signature_for_transient, transient_queue_ops::agent_from_queued_signature};
use jefe::domain::{AgentId, AgentKind, Repository, RepositoryId};
use tempfile::TempDir;

fn transient_repository(kind: AgentKind) -> (TempDir, Repository) {
    let root =
        tempfile::tempdir().unwrap_or_else(|error| panic!("create temp repository: {error}"));
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        "Repo".to_owned(),
        "repo".to_owned(),
        root.path().join("repo"),
    );
    repository.default_agent_kind = kind;
    (root, repository)
}

#[test]
fn transient_signature_copies_repository_llxprt_mode_flags() {
    let (_root, mut repository) = transient_repository(AgentKind::Llxprt);
    repository.default_llxprt_mode_flags = vec!["--yolo".to_owned(), "--fast".to_owned()];
    let work_dir = repository.effective_transient_dir().join("transient");

    let signature = launch_signature_for_transient(&repository, &work_dir);

    assert_eq!(signature.mode_flags, vec!["--yolo", "--fast"]);
}

#[test]
fn transient_signature_enables_code_puppy_yolo_by_default() {
    let (_root, repository) = transient_repository(AgentKind::CodePuppy);
    assert_eq!(repository.default_code_puppy_yolo, Some(true));
    let work_dir = repository.effective_transient_dir().join("transient");

    let signature = launch_signature_for_transient(&repository, &work_dir);

    assert_eq!(signature.code_puppy_yolo, Some(true));
}

#[test]
fn dequeued_agent_retains_the_queued_launch_snapshot() {
    for kind in [AgentKind::Llxprt, AgentKind::CodePuppy] {
        let (_root, mut repository) = transient_repository(kind);
        repository.default_profile = "queued-profile".to_owned();
        repository.default_llxprt_mode_flags = vec!["--queued-yolo".to_owned()];
        repository.default_code_puppy_yolo = Some(true);
        let signature = launch_signature_for_transient(
            &repository,
            &repository.effective_transient_dir().join("queued"),
        );

        repository.default_profile = "edited-profile".to_owned();
        repository.default_llxprt_mode_flags.clear();
        repository.default_code_puppy_yolo = None;

        let agent = agent_from_queued_signature(
            AgentId("transient-317".to_owned()),
            repository.id.clone(),
            &repository,
            &signature,
        );

        assert_eq!(agent.profile, signature.profile);
        assert_eq!(agent.mode_flags, signature.mode_flags);
        assert_eq!(agent.code_puppy_yolo, signature.code_puppy_yolo);
        assert_eq!(agent.agent_kind, signature.agent_kind);
        assert_eq!(agent.llxprt_version, signature.llxprt_version);
    }
}

#[test]
fn transient_signature_preserves_runtime_specific_yolo_opt_out() {
    let (_root, mut repository) = transient_repository(AgentKind::CodePuppy);
    repository.default_llxprt_mode_flags.clear();
    repository.default_code_puppy_yolo = None;
    let work_dir = repository.effective_transient_dir().join("transient");

    let signature = launch_signature_for_transient(&repository, &work_dir);

    assert!(signature.mode_flags.is_empty());
    assert_eq!(signature.code_puppy_yolo, None);
}
