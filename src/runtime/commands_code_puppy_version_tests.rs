//! Code Puppy version-specific launch-plan contracts.

use super::*;
use crate::domain::SandboxEngine;

fn base_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: Some(false),
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
        llxprt_version: None,
    }
}

#[test]
fn code_puppy_blank_version_keeps_direct_launch_plan_exact() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "  \n  ".to_owned();
    let plan = local_launch_plan(&signature);
    assert_eq!(
        plan.executable,
        AgentExecutableTarget::Agent(AgentKind::CodePuppy)
    );
    assert_eq!(plan.args, vec!["-i", "--yolo", "false"]);
}

#[test]
fn code_puppy_pinned_version_wraps_unchanged_inner_args_with_structural_uvx_argv() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "  0.0.361-rc1  ".to_owned();
    signature.code_puppy_model = "puppy-pro".to_owned();
    signature.code_puppy_yolo = Some(true);
    let plan = local_launch_plan(&signature);
    assert_eq!(plan.executable, AgentExecutableTarget::Uvx);
    assert_eq!(
        plan.args,
        vec![
            "--from",
            "code-puppy==0.0.361-rc1",
            "code-puppy",
            "-i",
            "--model",
            "puppy-pro",
            "--yolo",
            "true",
        ]
    );
}

#[test]
fn code_puppy_hostile_version_remains_one_local_and_remote_argument() {
    let version = "one space';$(touch nope)`touch nope`\nnext";
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = format!("  {version}  ");
    let local = local_launch_plan(&signature);
    assert_eq!(
        local.args,
        vec![
            "--from".to_owned(),
            format!("code-puppy=={version}"),
            "code-puppy".to_owned(),
            "-i".to_owned(),
            "--yolo".to_owned(),
            "false".to_owned(),
        ]
    );
    let remote = remote_launch_argv(&signature, None)
        .unwrap_or_else(|error| panic!("remote uvx plan: {error}"));
    assert_eq!(remote.executable, "uvx");
    assert_eq!(remote.args, local.args);
}

#[cfg(unix)]
#[test]
fn pinned_remote_shell_preserves_hostile_version_as_one_argument() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let capture = directory.path().join("argv.bin");
    let injected = directory.path().join("injected");
    let version = format!(
        "one space';$(touch {})`touch {}`\nnext",
        injected.display(),
        injected.display()
    );
    let uvx = directory.path().join("uvx");
    std::fs::write(&uvx, "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$CAPTURE\"\n")
        .unwrap_or_else(|error| panic!("write uvx fixture: {error}"));
    std::fs::set_permissions(&uvx, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod uvx fixture: {error}"));

    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = version.clone();
    let plan = remote_launch_argv(&signature, None)
        .unwrap_or_else(|error| panic!("remote uvx plan: {error}"));
    let path = std::env::join_paths(std::iter::once(directory.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap_or_else(|error| panic!("fixture PATH: {error}"));
    let output = std::process::Command::new("sh")
        .args(["-c", &remote_cli_command(&plan.executable, &plan.args)])
        .env("PATH", path)
        .env("CAPTURE", &capture)
        .output()
        .unwrap_or_else(|error| panic!("execute uvx fixture: {error}"));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(capture).unwrap_or_else(|error| panic!("read capture: {error}"));
    let actual = bytes
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual, plan.args);
    assert_eq!(actual[1], format!("code-puppy=={version}"));
    assert!(!injected.exists());
}

#[test]
fn pinned_remote_launch_bypasses_global_code_puppy_resolution() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "0.0.361".to_owned();
    signature.remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "builder".to_owned(),
        host: "example.test".to_owned(),
        ..crate::domain::RemoteRepositorySettings::default()
    };
    let command = build_remote_launch_command("pinned", &signature.work_dir, &signature)
        .unwrap_or_else(|error| panic!("pinned remote command: {error}"));
    assert!(command.contains("uvx"));
    assert!(command.contains("code-puppy==0.0.361"));
    assert!(!command.contains("command -v code-puppy"));
}

#[test]
fn code_puppy_latest_sentinel_produces_bare_uvx_package_spec() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "latest".to_owned();
    let plan = local_launch_plan(&signature);
    assert_eq!(plan.executable, AgentExecutableTarget::Uvx);
    // Bare package — no "==latest" suffix, which uv would reject
    assert_eq!(
        plan.args,
        vec![
            "--from",
            "code-puppy",
            "code-puppy",
            "-i",
            "--yolo",
            "false",
        ]
    );
}

#[test]
fn code_puppy_latest_sentinel_case_insensitive_produces_bare_spec() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "Latest".to_owned();
    let plan = local_launch_plan(&signature);
    assert_eq!(plan.executable, AgentExecutableTarget::Uvx);
    assert_eq!(
        plan.args,
        vec![
            "--from",
            "code-puppy",
            "code-puppy",
            "-i",
            "--yolo",
            "false",
        ]
    );
}

#[test]
fn code_puppy_latest_nightly_sentinel_produces_bare_uvx_package_spec() {
    // PyPI has no nightly channel for code-puppy, so both sentinels resolve
    // to the bare package name
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "latest nightly".to_owned();
    let plan = local_launch_plan(&signature);
    assert_eq!(plan.executable, AgentExecutableTarget::Uvx);
    assert_eq!(
        plan.args,
        vec![
            "--from",
            "code-puppy",
            "code-puppy",
            "-i",
            "--yolo",
            "false",
        ]
    );
}

#[test]
fn code_puppy_latest_sentinel_remote_uses_bare_package_spec() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "latest".to_owned();
    signature.remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "builder".to_owned(),
        host: "example.test".to_owned(),
        ..crate::domain::RemoteRepositorySettings::default()
    };
    let command = build_remote_launch_command("latest", &signature.work_dir, &signature)
        .unwrap_or_else(|error| panic!("latest remote command: {error}"));
    assert!(command.contains("uvx"));
    assert!(command.contains("code-puppy"));
    // No "==" suffix — bare package for sentinel
    assert!(!command.contains("code-puppy=="));
}

#[test]
fn code_puppy_latest_nightly_sentinel_remote_uses_bare_package_spec() {
    let mut signature = base_signature();
    signature.agent_kind = AgentKind::CodePuppy;
    signature.code_puppy_version = "latest nightly".to_owned();
    signature.remote = crate::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "builder".to_owned(),
        host: "example.test".to_owned(),
        ..crate::domain::RemoteRepositorySettings::default()
    };
    let command = build_remote_launch_command("latest nightly", &signature.work_dir, &signature)
        .unwrap_or_else(|error| panic!("latest nightly remote command: {error}"));
    assert!(command.contains("uvx"));
    assert!(command.contains("code-puppy"));
    assert!(!command.contains("code-puppy=="));
}
