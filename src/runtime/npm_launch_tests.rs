use super::super::agent_executable::AgentExecutableTarget;
use super::*;
use crate::domain::{LlxprtNpmPackageSelector, SandboxEngine};

fn signature(selector: Option<&str>) -> LaunchSignature {
    LaunchSignature {
        work_dir: Path::new("/tmp/work").to_path_buf(),
        profile: "review profile".to_owned(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: Some(false),
        code_puppy_quick_resume: false,
        mode_flags: vec!["--yolo".to_owned(), "prompt with spaces".to_owned()],
        llxprt_debug: "trace;safe".to_owned(),
        pass_continue: true,
        sandbox_enabled: true,
        sandbox_engine: SandboxEngine::Docker,
        sandbox_flags: "--network none".to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
        llxprt_version: selector.and_then(LlxprtNpmPackageSelector::normalize),
    }
}

#[test]

fn direct_local_plan_remains_exact() {
    let plan = local_launch_plan(&signature(None));
    assert_eq!(
        plan.executable,
        AgentExecutableTarget::Agent(AgentKind::Llxprt)
    );
    assert_eq!(
        plan.args,
        vec![
            "--profile-load",
            "review profile",
            "--yolo",
            "prompt with spaces",
            "--continue",
            "--sandbox",
            "--sandbox-engine",
            "docker",
        ]
    );
}

#[test]

fn nightly_local_plan_prefixes_complete_existing_llxprt_argv() {
    let nightly = "0.10.0-nightly.260712.21cb698b6";
    let plan = local_launch_plan(&signature(Some(nightly)));
    assert_eq!(plan.executable, AgentExecutableTarget::Npm);
    assert_eq!(
        plan.args,
        vec![
            "exec",
            "--yes",
            "--package=@vybestack/llxprt-code@0.10.0-nightly.260712.21cb698b6",
            "--",
            "llxprt",
            "--profile-load",
            "review profile",
            "--yolo",
            "prompt with spaces",
            "--continue",
            "--sandbox",
            "--sandbox-engine",
            "docker",
        ]
    );
    assert!(
        plan.env
            .iter()
            .any(|pair| pair == &("LLXPRT_DEBUG".to_owned(), "trace;safe".to_owned()))
    );
    assert!(
        plan.env
            .iter()
            .any(|pair| pair == &("SANDBOX_FLAGS".to_owned(), "--network none".to_owned()))
    );
    local_metacharacter_selector_is_one_argument();
    code_puppy_ignores_dormant_selector();
    remote_versioned_argv_is_complete_and_structural();
}

fn local_metacharacter_selector_is_one_argument() {
    let plan = local_launch_plan(&signature(Some("1.0;$(touch nope)`touch no`\nnext")));
    assert_eq!(
        plan.args[2],
        "--package=@vybestack/llxprt-code@1.0;$(touch nope)`touch no`\nnext"
    );
    assert_eq!(plan.args.len(), 13);
    remote_dynamic_argv_is_shell_escaped_exactly_once();
}

fn code_puppy_ignores_dormant_selector() {
    let mut sig = signature(Some("nightly"));
    sig.agent_kind = AgentKind::CodePuppy;
    sig.mode_flags.clear();
    sig.sandbox_enabled = false;
    let plan = local_launch_plan(&sig);
    assert_eq!(
        plan.executable,
        AgentExecutableTarget::Agent(AgentKind::CodePuppy)
    );
    assert_eq!(plan.args, vec!["-i", "--yolo", "false"]);
}

fn remote_versioned_argv_is_complete_and_structural() {
    let plan = remote_launch_argv(&signature(Some("nightly")), None)
        .unwrap_or_else(|error| panic!("versioned plan: {error}"));
    assert_eq!(plan.executable, "npm");
    assert_eq!(
        plan.args,
        local_launch_plan(&signature(Some("nightly"))).args
    );
}

fn remote_dynamic_argv_is_shell_escaped_exactly_once() {
    let values = [
        "with space",
        "single'quote",
        "semi;colon",
        "$(touch injected)",
        "`touch injected2`",
        "line\nbreak",
    ];
    for value in values {
        let mut sig = signature(Some(value));
        sig.mode_flags = vec![value.to_owned()];
        let plan =
            remote_launch_argv(&sig, None).unwrap_or_else(|error| panic!("remote argv: {error}"));
        let command = remote_cli_command(&plan.executable, &plan.args);
        assert!(command.contains(&shell_escape_single(&format!(
            "--package=@vybestack/llxprt-code@{value}"
        ))));
        assert!(command.contains(&shell_escape_single(value)));
    }
}

#[cfg(unix)]
#[test]
fn remote_shell_receives_adversarial_selector_as_exactly_one_argument() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let capture = directory.path().join("argv.bin");
    let injected_one = directory.path().join("injected-one");
    let injected_two = directory.path().join("injected-two");
    let selector = format!(
        "with space's;$(touch {})`touch {}`\nline",
        injected_one.display(),
        injected_two.display()
    );
    let npm = directory.path().join("npm");
    std::fs::write(&npm, "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$CAPTURE\"\n")
        .unwrap_or_else(|error| panic!("write npm fixture: {error}"));
    std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm fixture: {error}"));

    let sig = signature(Some(&selector));
    let plan =
        remote_launch_argv(&sig, None).unwrap_or_else(|error| panic!("remote argv: {error}"));
    let command = remote_cli_command(&plan.executable, &plan.args);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let joined_path = std::env::join_paths(
        std::iter::once(directory.path().to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap_or_else(|error| panic!("fixture PATH: {error}"));
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("PATH", joined_path)
        .env("CAPTURE", &capture)
        .output()
        .unwrap_or_else(|error| panic!("execute shell fixture: {error}"));
    assert!(
        output.status.success(),
        "fixture stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let bytes = std::fs::read(capture).unwrap_or_else(|error| panic!("read argv capture: {error}"));
    let actual = bytes
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual, plan.args);
    assert!(
        !injected_one.exists(),
        "command substitution must not execute"
    );
    assert!(!injected_two.exists(), "backticks must not execute");
}

#[cfg(windows)]
#[test]
fn windows_npm_cmd_bypasses_cmd_and_preserves_adversarial_argv() {
    use super::super::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver};

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm = directory.path().join("npm.cmd");
    let node = directory.path().join("node.exe");
    let cli = directory.path().join("node_modules/npm/bin/npm-cli.js");
    std::fs::write(&npm, "@echo off\r\nexit /b 99\r\n")
        .unwrap_or_else(|error| panic!("write npm fixture: {error}"));
    std::fs::write(&node, b"fixture").unwrap_or_else(|error| panic!("write node fixture: {error}"));
    std::fs::create_dir_all(cli.parent().unwrap_or_else(|| directory.path()))
        .unwrap_or_else(|error| panic!("create npm cli directory: {error}"));
    std::fs::write(&cli, b"fixture").unwrap_or_else(|error| panic!("write npm cli: {error}"));
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(std::ffi::OsString::from(".CMD")),
    );
    let executable = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .unwrap_or_else(|error| panic!("resolve npm fixture: {error}"));
    let selector = std::ffi::OsString::from("--package=@scope/pkg@a b&c|d<e>f^g%h!i(j)");
    let command = super::super::agent_launcher::command_for_executable(
        &executable,
        &[std::ffi::OsString::from("exec"), selector.clone()],
    );
    let canonical_node =
        std::fs::canonicalize(&node).unwrap_or_else(|error| panic!("canonical node: {error}"));
    let canonical_cli =
        std::fs::canonicalize(&cli).unwrap_or_else(|error| panic!("canonical cli: {error}"));
    let args = command.get_args().collect::<Vec<_>>();
    assert_eq!(command.get_program(), canonical_node);
    assert_eq!(
        args,
        [
            canonical_cli.as_os_str(),
            std::ffi::OsStr::new("exec"),
            selector.as_os_str()
        ]
    );
    assert!(!args.iter().any(|arg| *arg == npm.as_os_str()));
}

#[cfg(windows)]
#[test]
fn windows_noncanonical_npm_cmd_is_rejected_before_command_construction() {
    use super::super::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver};

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    std::fs::write(directory.path().join("npm.cmd"), "@echo off\r\n")
        .unwrap_or_else(|error| panic!("write npm fixture: {error}"));
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(std::ffi::OsString::from(".CMD")),
    );

    let error = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .err()
        .unwrap_or_else(|| panic!("noncanonical npm wrapper must fail"));
    assert!(error.to_string().contains("official Node.js layout"));
}

#[test]
fn latest_sentinel_produces_npm_latest_dist_tag_spec() {
    let plan = local_launch_plan(&signature(Some("latest")));
    assert_eq!(plan.executable, AgentExecutableTarget::Npm);
    // npm resolves @vybestack/llxprt-code@latest natively
    assert_eq!(plan.args[2], "--package=@vybestack/llxprt-code@latest");
}

#[test]
fn latest_nightly_sentinel_produces_npm_nightly_dist_tag_spec() {
    // User types "latest nightly", npm dist-tag is "nightly"
    let plan = local_launch_plan(&signature(Some("latest nightly")));
    assert_eq!(plan.executable, AgentExecutableTarget::Npm);
    assert_eq!(plan.args[2], "--package=@vybestack/llxprt-code@nightly");
}

#[test]
fn latest_sentinel_remote_uses_latest_dist_tag() {
    let plan = remote_launch_argv(&signature(Some("latest")), None)
        .unwrap_or_else(|error| panic!("latest remote plan: {error}"));
    assert_eq!(plan.args[2], "--package=@vybestack/llxprt-code@latest");
}

#[test]
fn latest_nightly_sentinel_remote_uses_nightly_dist_tag() {
    let plan = remote_launch_argv(&signature(Some("latest nightly")), None)
        .unwrap_or_else(|error| panic!("nightly remote plan: {error}"));
    assert_eq!(plan.args[2], "--package=@vybestack/llxprt-code@nightly");
}
