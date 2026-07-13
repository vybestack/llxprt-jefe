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
fn probe_plan_versioned_llxprt_probes_npm_not_llxprt() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
        "0.9.0",
    );
    let command = probe_command(&argv);
    assert!(command.contains("npm"), "must check npm: {command}");
    assert!(
        !command.contains("llxprt"),
        "must not check llxprt: {command}"
    );
}

#[test]
fn probe_plan_versioned_llxprt_does_not_check_path_local_binary() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
        "0.10.0-nightly.260712.21cb698b6",
    );
    let command = probe_command(&argv);
    assert!(command.contains("npm"), "must check npm: {command}");
    assert!(
        !command.contains("node_modules"),
        "must not check node_modules: {command}"
    );
}

#[test]
fn probe_plan_blank_llxprt_still_probes_llxprt_binary() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
        "",
    );
    let command = probe_command(&argv);
    assert!(command.contains("llxprt"), "must check llxprt: {command}");
    assert!(!command.contains("npm"), "must not check npm: {command}");
}

#[test]
fn probe_plan_whitespace_only_version_probes_llxprt_not_npm() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
        "   ",
    );
    let command = probe_command(&argv);
    assert!(command.contains("llxprt") && !command.contains("npm"));
}

#[test]
fn require_runtime_versioned_llxprt_local_passes_with_npm_no_llxprt() {
    let result = require_runtime_available(
        &WorkTarget::Local,
        Path::new("/tmp/work"),
        AgentKind::Llxprt,
        &[AgentKind::CodePuppy],
        "0.9.0",
        true,
    );
    assert!(result.is_ok());
}

#[test]
fn require_runtime_versioned_llxprt_local_fails_without_npm() {
    let result = require_runtime_available(
        &WorkTarget::Local,
        Path::new("/tmp/work"),
        AgentKind::Llxprt,
        &[AgentKind::Llxprt],
        "0.9.0",
        false,
    );
    let Err(error) = result else {
        panic!("versioned LLxprt without npm must fail");
    };
    assert!(error.contains("npm"), "error must reference npm: {error}");
}
