use super::*;
use crate::domain::{AgentStatus, RemoteRepositorySettings, Repository, RepositoryId};

fn local_repository() -> Repository {
    Repository {
        id: RepositoryId("repo-1".to_owned()),
        name: "Repo 1".to_owned(),
        slug: "repo-1".to_owned(),
        base_dir: std::path::PathBuf::from("/tmp/repo-1"),
        default_profile: String::new(),
        github_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        agent_ids: Vec::new(),
    }
}

fn remote_repository() -> Repository {
    Repository {
        remote: RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "example.com".to_owned(),
            run_as_user: "acoliver".to_owned(),
            setup_env_default: false,
        },
        ..local_repository()
    }
}

fn params<'a>(
    repository: &'a Repository,
    name: &'a str,
    work_dir: &'a str,
) -> CreateAgentParams<'a> {
    CreateAgentParams {
        repository,
        name,
        description: "",
        work_dir,
        profile: "",
        agent_kind: "LLxprt",
        mode: "",
        llxprt_debug: "",
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman",
        sandbox_flags: "",
        shortcut_slot: None,
        next_display_index: 1,
    }
}

fn created(params: CreateAgentParams<'_>) -> Agent {
    let Some(agent) = create_agent(params) else {
        panic!("agent should be created");
    };
    agent
}

#[test]
fn create_agent_rejects_blank_name() {
    let repo = local_repository();
    assert!(create_agent(params(&repo, "   ", "/tmp/agent")).is_none());
}

#[test]
fn create_agent_rejects_blank_work_dir() {
    let repo = local_repository();
    assert!(create_agent(params(&repo, "Agent", "   \t ")).is_none());
}

#[test]
fn create_agent_sets_running_status() {
    let repo = local_repository();
    let agent = created(params(&repo, "Agent", "/tmp/agent"));
    assert_eq!(agent.status, AgentStatus::Running);
}

#[test]
fn create_agent_trims_name() {
    let repo = local_repository();
    let agent = created(params(&repo, "  Agent One  ", "/tmp/agent"));
    assert_eq!(agent.name, "Agent One");
}

#[test]
fn create_agent_normalizes_profile() {
    let repo = local_repository();

    let blank = created(CreateAgentParams {
        profile: "  ",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(blank.profile, "");

    let brackets = created(CreateAgentParams {
        profile: "[]",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(brackets.profile, "");

    let custom = created(CreateAgentParams {
        profile: "custom",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(custom.profile, "custom");
}

#[test]
fn create_agent_normalizes_mode_flags() {
    let repo = local_repository();

    let default_mode = created(params(&repo, "Agent", "/tmp/agent"));
    assert_eq!(default_mode.mode_flags, vec!["--yolo".to_owned()]);

    let explicit = created(CreateAgentParams {
        mode: "--fast --verbose",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        explicit.mode_flags,
        vec!["--fast".to_owned(), "--verbose".to_owned()]
    );
}

#[test]
fn create_agent_normalizes_sandbox_engine_via_platform() {
    let repo = local_repository();
    let caps = PlatformCapabilities::current();
    let expected = SandboxEngine::from_form_value("docker")
        .and_then(|engine| caps.normalize_engine(engine))
        .unwrap_or_default();

    let agent = created(CreateAgentParams {
        sandbox_engine: "docker",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(agent.sandbox_engine, expected);
}

#[test]
fn create_agent_expands_tilde_for_local_repository() {
    let Some(home) = std::env::var_os("HOME") else {
        // No HOME set in this environment; tilde expansion is a no-op, which is
        // covered indirectly elsewhere. Skip the home-dependent assertion.
        return;
    };
    let home = home.to_string_lossy().into_owned();
    let repo = local_repository();
    let agent = created(params(&repo, "Agent", "~/work/agent"));
    assert_eq!(
        agent.work_dir,
        std::path::PathBuf::from(format!("{home}/work/agent"))
    );
}

#[test]
fn create_agent_keeps_work_dir_verbatim_for_remote_repository() {
    let repo = remote_repository();
    let agent = created(params(&repo, "Agent", "~/work/agent"));
    assert_eq!(agent.work_dir, std::path::PathBuf::from("~/work/agent"));
}
