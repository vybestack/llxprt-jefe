//! Launch-signature tests for repository transient options (issue #317).

use super::{launch_signature_for_transient, transient_queue_ops::agent_from_queued_signature};
use jefe::domain::{AgentId, AgentKind, Repository, RepositoryId};

#[test]
fn transient_signature_copies_repository_llxprt_mode_flags() {
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo-317".into(),
    );
    repository.default_agent_kind = AgentKind::Llxprt;
    repository.default_llxprt_mode_flags = vec!["--yolo".to_owned(), "--fast".to_owned()];

    let signature = launch_signature_for_transient(&repository, "/tmp/transient-317".as_ref());

    assert_eq!(signature.mode_flags, vec!["--yolo", "--fast"]);
}

#[test]
fn transient_signature_enables_code_puppy_yolo_by_default() {
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo-317".into(),
    );
    repository.default_agent_kind = AgentKind::CodePuppy;

    let signature = launch_signature_for_transient(&repository, "/tmp/transient-317".as_ref());

    assert_eq!(signature.code_puppy_yolo, Some(true));
}

#[test]
fn dequeued_agent_retains_the_queued_launch_snapshot() {
    for kind in [AgentKind::Llxprt, AgentKind::CodePuppy] {
        let mut repository = Repository::new(
            RepositoryId("repo-317".to_owned()),
            "Repo".to_owned(),
            "repo".to_owned(),
            "/tmp/repo-317".into(),
        );
        repository.default_agent_kind = kind;
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
    let mut repository = Repository::new(
        RepositoryId("repo-317".to_owned()),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo-317".into(),
    );
    repository.default_agent_kind = AgentKind::CodePuppy;
    repository.default_llxprt_mode_flags.clear();
    repository.default_code_puppy_yolo = None;

    let signature = launch_signature_for_transient(&repository, "/tmp/transient-317".as_ref());

    assert!(signature.mode_flags.is_empty());
    assert_eq!(signature.code_puppy_yolo, None);
}
