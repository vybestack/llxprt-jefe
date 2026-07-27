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
    assert!(plan.managed_bin_dir.is_none());
}

#[test]

fn nightly_local_plan_runs_cached_binary_directly() {
    // Issue #425: local versioned launches run the cached `llxprt` binary
    // directly from the jefe-managed install dir instead of `npm exec`. The
    // plan's executable is the agent target (resolved from the managed bin
    // dir), the args are the inner llxprt argv (no `exec --package` wrapper),
    // and `managed_bin_dir` points at the cache's `node_modules/.bin`.
    let nightly = "0.10.0-nightly.260712.21cb698b6";
    let plan = local_launch_plan(&signature(Some(nightly)));
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
    let bin_dir = plan
        .managed_bin_dir
        .as_ref()
        .unwrap_or_else(|| panic!("managed bin dir must be set for a versioned local launch"));
    // Assert the full expected suffix so the relative order of cache-root ->
    // version dir -> node_modules/.bin is fixed, not just that each component
    // appears somewhere.
    assert!(
        bin_dir.ends_with(std::path::Path::new(
            "llxprt-versions/0.10.0-nightly.260712.21cb698b6/node_modules/.bin"
        )),
        "managed bin dir must end in the version-cache .bin path: {}",
        bin_dir.display()
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
    local_metacharacter_selector_dir_name_is_filesystem_safe();
    code_puppy_ignores_dormant_selector();
    remote_versioned_argv_is_complete_and_structural();
}

fn local_metacharacter_selector_dir_name_is_filesystem_safe() {
    // Issue #425: a selector with shell metacharacters is not passed to a
    // shell on the local path (the cached binary is invoked directly), but
    // the version_dir_name still must be filesystem-safe. The plan's
    // managed_bin_dir derives from the sanitized dir name.
    let sel = LlxprtNpmPackageSelector::normalize("1.0;$(touch nope)`touch no`\nnext")
        .unwrap_or_else(|| panic!("selector normalizes"));
    let dir = sel.version_dir_name();
    assert!(
        !dir.contains('/')
            && !dir.contains('\\')
            && !dir.contains(' ')
            && !dir.contains(':')
            && !dir.contains('?')
            && !dir.contains('*')
            && !dir.starts_with('.')
            && !dir.starts_with(' '),
        "version dir name must be filesystem-safe: {dir}"
    );
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
    assert!(plan.managed_bin_dir.is_none());
}

fn remote_versioned_argv_is_complete_and_structural() {
    // Remote versioned launches keep the `npm exec` form (issue #425
    // non-goal: jefe has no managed install on the remote host).
    let plan = remote_launch_argv(&signature(Some("nightly")), None)
        .unwrap_or_else(|error| panic!("versioned plan: {error}"));
    assert_eq!(plan.executable, "npm");
    let local = local_launch_plan(&signature(Some("nightly")));
    // The local plan runs the binary directly (no `exec --package` prefix);
    // the remote plan keeps the npm-exec wrapper. They must NOT match.
    assert_ne!(
        plan.args, local.args,
        "remote argv keeps npm exec; local argv runs the cached binary"
    );
    assert_eq!(plan.args[0], "exec");
    assert_eq!(plan.args[1], "--yes");
    // Explicitly assert the package spec so a regression in the npm-exec
    // wrapper is caught even though the local plan no longer uses it.
    assert_eq!(
        plan.args[2],
        format!(
            "--package={}",
            LlxprtNpmPackageSelector::normalize("nightly")
                .unwrap_or_else(|| panic!("selector"))
                .package_spec()
        )
    );
    assert_eq!(plan.args[3], "--");
    assert_eq!(plan.args[4], "llxprt");
}

fn remote_dynamic_argv_is_shell_escaped_exactly_once() {
    // Issue #403: internal whitespace is now stripped by normalization, so
    // the selectors used for shell-escape verification are whitespace-free.
    let values = [
        "withspace",
        "single'quote",
        "semi;colon",
        "$(touchinjected)",
        "`touchinjected2`",
        "linebreak",
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
    // Issue #403: internal whitespace is stripped by normalization, so the
    // selector that reaches the shell is whitespace-free. The shell-escape
    // invariant still holds: the selector is exactly one argument and
    // command substitution / backticks do not execute.
    let selector = format!(
        "withspace's;$(touch{})`touch{}`line",
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
fn latest_sentinel_local_runs_cached_binary_with_nightly_dir_name() {
    // Issue #425: `latest` maps to the `latest` dist-tag for the install pin,
    // and the managed bin dir uses the sanitized dir name.
    let plan = local_launch_plan(&signature(Some("latest")));
    assert_eq!(
        plan.executable,
        AgentExecutableTarget::Agent(AgentKind::Llxprt)
    );
    let bin_dir = plan
        .managed_bin_dir
        .as_ref()
        .unwrap_or_else(|| panic!("managed bin dir set for latest"));
    assert!(
        bin_dir.ends_with(std::path::Path::new("latest/node_modules/.bin")),
        "latest selector dir name: {}",
        bin_dir.display()
    );
}

#[test]
fn latest_nightly_sentinel_local_runs_cached_binary_with_nightly_dir_name() {
    // User types "latest nightly", npm dist-tag is "nightly"
    let plan = local_launch_plan(&signature(Some("latest nightly")));
    assert_eq!(
        plan.executable,
        AgentExecutableTarget::Agent(AgentKind::Llxprt)
    );
    let bin_dir = plan
        .managed_bin_dir
        .as_ref()
        .unwrap_or_else(|| panic!("managed bin dir set for nightly"));
    assert!(
        bin_dir.ends_with(std::path::Path::new("nightly/node_modules/.bin")),
        "nightly selector dir name: {}",
        bin_dir.display()
    );
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

#[cfg(windows)]
#[test]
fn windows_official_llxprt_script_plan_launches_bun_with_entrypoint_first_argument() {
    use super::super::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver};
    use std::ffi::{OsStr, OsString};

    const MARKER: &str = "LLXPRT_NATIVE_LAUNCHER owned by @vybestack/llxprt-code";
    const BUN_REL: &str = "node_modules/@vybestack/llxprt-code/node_modules/bun/bin/bun.exe";
    const ENTRY_REL: &str = "node_modules/@vybestack/llxprt-code/index.ts";

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let wrapper = directory.path().join("llxprt.cmd");
    std::fs::write(
        &wrapper,
        format!("@echo off\r\nrem {MARKER}\r\n").as_bytes(),
    )
    .unwrap_or_else(|error| panic!("write wrapper: {error}"));
    let bun = directory.path().join(BUN_REL);
    let entry = directory.path().join(ENTRY_REL);
    for path in [&bun, &entry] {
        std::fs::create_dir_all(path.parent().unwrap_or_else(|| directory.path()))
            .unwrap_or_else(|error| panic!("create fixture dir: {error}"));
        std::fs::write(path, b"fixture").unwrap_or_else(|error| panic!("write fixture: {error}"));
    }
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(std::ffi::OsString::from(".CMD")),
    );
    let executable = resolver
        .resolve(crate::domain::AgentKind::Llxprt)
        .unwrap_or_else(|error| panic!("resolve official wrapper: {error}"));
    let prompt = OsString::from("x".repeat(8_092));
    let command = super::super::agent_launcher::command_for_executable(
        &executable,
        &[
            OsString::from("--profile-load"),
            OsString::from("p"),
            prompt,
        ],
    );
    let canonical_bun =
        std::fs::canonicalize(&bun).unwrap_or_else(|error| panic!("canonical bun: {error}"));
    let canonical_entry =
        std::fs::canonicalize(&entry).unwrap_or_else(|error| panic!("canonical entry: {error}"));
    let args = command.get_args().collect::<Vec<_>>();
    assert_eq!(command.get_program(), canonical_bun.as_path());
    assert_eq!(args.len(), 4);
    assert_eq!(args[0], canonical_entry.as_os_str());
    assert_eq!(args[1], OsStr::new("--profile-load"));
    assert_eq!(args[2], OsStr::new("p"));
    assert_eq!(args[3].len(), 8_092);
    assert!(
        !args.iter().any(|arg| *arg == wrapper.as_os_str()),
        "wrapper must not appear in argv"
    );
    assert!(
        !command
            .get_program()
            .to_str()
            .is_some_and(|program| program.eq_ignore_ascii_case("cmd.exe"))
    );
}
