use super::issue_prep::WorkTarget;
use super::remote_probe::{plan_remote_probe, require_runtime_available};
use jefe::domain::{AgentKind, RemoteRepositorySettings};
use std::path::Path;

fn valid_remote() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "build.example.com".to_owned(),
        ..RemoteRepositorySettings::default()
    }
}

fn probe_command(argv: &[String]) -> &str {
    argv.iter()
        .find(|argument| argument.contains("command -v"))
        .map_or_else(|| panic!("must have command -v: {argv:?}"), String::as_str)
}

#[test]
fn versioned_llxprt_probe_requires_npm_not_llxprt_or_path_local_binary() {
    for selector in ["0.9.0", "0.10.0-nightly.260712.21cb698b6"] {
        let argv = plan_remote_probe(
            &valid_remote(),
            Path::new("/home/ubuntu/work"),
            AgentKind::Llxprt,
            selector,
        );
        let command = probe_command(&argv);
        assert!(command.contains("npm"), "must check npm: {command}");
        assert!(
            !command.contains("llxprt") && !command.contains("node_modules"),
            "must not check direct or path-local LLxprt: {command}"
        );
    }
}

#[test]
fn blank_llxprt_probe_requires_direct_llxprt_not_npm() {
    for selector in ["", "   "] {
        let argv = plan_remote_probe(
            &valid_remote(),
            Path::new("/home/ubuntu/work"),
            AgentKind::Llxprt,
            selector,
        );
        let command = probe_command(&argv);
        assert!(command.contains("llxprt"), "must check llxprt: {command}");
        assert!(!command.contains("npm"), "must not check npm: {command}");
    }
}

#[test]
fn local_versioned_llxprt_availability_depends_on_npm() {
    let available = require_runtime_available(
        &WorkTarget::Local,
        Path::new("/tmp/work"),
        AgentKind::Llxprt,
        &[AgentKind::CodePuppy],
        "0.9.0",
        true,
    );
    assert!(available.is_ok());

    let unavailable = require_runtime_available(
        &WorkTarget::Local,
        Path::new("/tmp/work"),
        AgentKind::Llxprt,
        &[AgentKind::Llxprt],
        "0.9.0",
        false,
    );
    let Err(error) = unavailable else {
        panic!("versioned LLxprt without npm must fail");
    };
    assert!(error.contains("npm"), "error must reference npm: {error}");
}
