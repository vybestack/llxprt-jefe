//! Behavioral contracts for native agent executable resolution and launch planning.

use std::ffi::OsString;
use std::path::PathBuf;

use tempfile::TempDir;

use super::agent_executable::{
    AgentExecutablePlatform, AgentExecutableResolver, AgentExecutableTarget, AgentWrapperKind,
};
use crate::domain::AgentKind;

fn write_candidate(directory: &TempDir, name: &str) -> PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, b"fixture")
        .unwrap_or_else(|error| panic!("write executable fixture: {error}"));
    path
}

/// Build the expected canonical path the way `canonical_script_launch_plan`
/// stores it: canonicalized, then on Windows with the `\\?\` verbatim prefix
/// stripped (issue #432). Centralizing this keeps the test assertions aligned
/// with the production helper instead of re-implementing the strip.
#[cfg(windows)]
fn expected_canonical(path: PathBuf) -> PathBuf {
    let canonical = std::fs::canonicalize(&path)
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()));
    super::agent_executable::strip_verbatim_prefix(&canonical)
}

#[test]

fn windows_resolution_follows_pathext_directory_and_extension_order() {
    let first = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let second = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    write_candidate(&first, "llxprt.CMD");
    let first_exe = write_candidate(&first, "llxprt.exe");
    write_candidate(&second, "llxprt.COM");

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![first.path().to_path_buf(), second.path().to_path_buf()],
        Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
    );
    let executable = policy
        .resolve(AgentKind::Llxprt)
        .unwrap_or_else(|error| panic!("Windows candidate should resolve: {error}"));

    assert_eq!(executable.path(), first_exe);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::Direct);
}

#[test]

fn windows_resolution_classifies_all_supported_wrapper_forms() {
    for (name, expected) in [
        ("code-puppy.exe", AgentWrapperKind::Direct),
        ("code-puppy.com", AgentWrapperKind::Direct),
        ("code-puppy.cmd", AgentWrapperKind::CommandScript),
        ("code-puppy.bat", AgentWrapperKind::CommandScript),
        ("code-puppy.ps1", AgentWrapperKind::PowerShellScript),
    ] {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
        let expected_path = write_candidate(&directory, name);
        let policy = AgentExecutableResolver::for_platform(
            AgentExecutablePlatform::Windows,
            vec![directory.path().to_path_buf()],
            Some(OsString::from(".EXE;.COM;.CMD;.BAT")),
        );
        let executable = policy
            .resolve(AgentKind::CodePuppy)
            .unwrap_or_else(|error| panic!("{name} should resolve: {error}"));
        assert_eq!(executable.path(), expected_path);
        assert_eq!(executable.wrapper_kind(), expected, "candidate {name}");
    }

    windows_npm_resolution_reuses_command_wrapper_policy();
    missing_npm_diagnostic_names_npm_remediation();
}

#[test]

fn windows_resolution_ignores_unsupported_files_and_reports_safe_remediation() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    write_candidate(&directory, "llxprt.js");
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".JS;.EXE;.CMD")),
    );

    let error = match resolver.resolve(AgentKind::Llxprt) {
        Ok(executable) => panic!("unsupported candidate resolved: {executable:?}"),
        Err(error) => error,
    };
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("LLxprt"), "diagnostic: {diagnostic}");
    assert!(diagnostic.contains(".exe, .com, .cmd, .bat, or .ps1"));
    assert!(!diagnostic.contains("prompt"));
}

#[cfg(unix)]
#[test]
fn unix_resolution_keeps_extensionless_executable_contract() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let executable = write_candidate(&directory, "llxprt");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("mark fixture executable: {error}"));
    write_candidate(&directory, "llxprt.exe");
    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![directory.path().to_path_buf()],
        None,
    );

    let agent_executable = policy
        .resolve(AgentKind::Llxprt)
        .unwrap_or_else(|error| panic!("Unix executable should resolve: {error}"));
    assert_eq!(agent_executable.path(), executable);
    assert_eq!(agent_executable.wrapper_kind(), AgentWrapperKind::Direct);
}

fn windows_npm_resolution_reuses_command_wrapper_policy() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let expected = write_candidate(&directory, "npm.cmd");
    let node = write_candidate(&directory, "node.exe");
    let cli = directory.path().join("node_modules/npm/bin/npm-cli.js");
    std::fs::create_dir_all(cli.parent().unwrap_or_else(|| directory.path()))
        .unwrap_or_else(|error| panic!("create npm fixture: {error}"));
    std::fs::write(&cli, b"fixture").unwrap_or_else(|error| panic!("write npm cli: {error}"));
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".EXE;.CMD")),
    );

    let executable = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .unwrap_or_else(|error| panic!("npm.cmd should resolve: {error}"));
    let Some(plan) = executable.script_launch_plan() else {
        panic!("npm.cmd must retain a canonical direct Node.js plan");
    };

    assert_eq!(executable.path(), expected);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert_eq!(executable.target(), AgentExecutableTarget::Npm);
    assert_eq!(
        plan.runtime(),
        expected_canonical(node),
        "runtime path must be the de-prefixed canonical form (issue #432)"
    );
    assert_eq!(
        plan.entrypoint(),
        expected_canonical(cli),
        "entrypoint path must be the de-prefixed canonical form (issue #432)"
    );
}

fn missing_npm_diagnostic_names_npm_remediation() {
    let resolver =
        AgentExecutableResolver::for_platform(AgentExecutablePlatform::Unix, Vec::new(), None);

    let error = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .err()
        .unwrap_or_else(|| panic!("missing npm should fail"));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("npm"), "diagnostic: {diagnostic}");
    assert!(diagnostic.contains("Node.js"), "diagnostic: {diagnostic}");
}

#[test]
fn uvx_resolves_with_supported_unix_and_windows_wrapper_policies() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let expected = write_candidate(&directory, "uvx");
        std::fs::set_permissions(&expected, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod uvx: {error}"));
        let policy = AgentExecutableResolver::for_platform(
            AgentExecutablePlatform::Unix,
            vec![directory.path().to_path_buf()],
            None,
        );
        let executable = policy
            .resolve_target(AgentExecutableTarget::Uvx)
            .unwrap_or_else(|error| panic!("resolve uvx: {error}"));
        assert_eq!(executable.path(), expected);
        assert_eq!(executable.wrapper_kind(), AgentWrapperKind::Direct);
    }

    for (name, wrapper) in [
        ("uvx.exe", AgentWrapperKind::Direct),
        ("uvx.cmd", AgentWrapperKind::CommandScript),
        ("uvx.bat", AgentWrapperKind::CommandScript),
        ("uvx.ps1", AgentWrapperKind::PowerShellScript),
    ] {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let expected = write_candidate(&directory, name);
        let policy = AgentExecutableResolver::for_platform(
            AgentExecutablePlatform::Windows,
            vec![directory.path().to_path_buf()],
            Some(OsString::from(".EXE;.CMD;.BAT")),
        );
        let executable = policy
            .resolve_target(AgentExecutableTarget::Uvx)
            .unwrap_or_else(|error| panic!("resolve {name}: {error}"));
        assert_eq!(executable.path(), expected);
        assert_eq!(executable.wrapper_kind(), wrapper);
    }
}

#[test]
fn missing_uvx_diagnostic_is_actionable() {
    let resolver =
        AgentExecutableResolver::for_platform(AgentExecutablePlatform::Unix, Vec::new(), None);
    let error = resolver
        .resolve_target(AgentExecutableTarget::Uvx)
        .err()
        .unwrap_or_else(|| panic!("missing uvx should fail"));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("uvx"));
    assert!(diagnostic.contains("uv"));
    assert!(diagnostic.contains("PATH"));
}

#[test]

fn windows_npm_resolution_rejects_noncanonical_command_wrapper() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    write_candidate(&directory, "npm.cmd");
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );

    let error = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .err()
        .unwrap_or_else(|| panic!("noncanonical npm.cmd must be rejected"));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("official Node.js layout"));
    assert!(diagnostic.contains("node.exe"));
    assert!(diagnostic.contains("npm-cli.js"));
}

const LLXPRT_WRAPPER_MARKER: &str = "LLXPRT_NATIVE_LAUNCHER owned by @vybestack/llxprt-code";
const LLXPRT_BUN_REL: &str = "node_modules/@vybestack/llxprt-code/node_modules/bun/bin/bun.exe";
const LLXPRT_ENTRYPOINT_REL: &str = "node_modules/@vybestack/llxprt-code/index.ts";

fn write_official_llxprt_wrapper(directory: &TempDir, include_bun: bool, include_entry: bool) {
    let wrapper = directory.path().join("llxprt.cmd");
    let body = format!(
        "@echo off\r\nrem {LLXPRT_WRAPPER_MARKER}\r\nrem official launcher\r\nexit /b 0\r\n"
    );
    std::fs::write(&wrapper, body.as_bytes())
        .unwrap_or_else(|error| panic!("write official wrapper: {error}"));
    if include_bun {
        let bun = directory.path().join(LLXPRT_BUN_REL);
        std::fs::create_dir_all(bun.parent().unwrap_or_else(|| directory.path()))
            .unwrap_or_else(|error| panic!("create bun dir: {error}"));
        std::fs::write(&bun, b"fixture")
            .unwrap_or_else(|error| panic!("write bun fixture: {error}"));
    }
    if include_entry {
        let entry = directory.path().join(LLXPRT_ENTRYPOINT_REL);
        std::fs::create_dir_all(entry.parent().unwrap_or_else(|| directory.path()))
            .unwrap_or_else(|error| panic!("create entry dir: {error}"));
        std::fs::write(&entry, b"fixture")
            .unwrap_or_else(|error| panic!("write entry fixture: {error}"));
    }
}

#[test]
fn windows_official_llxprt_wrapper_resolves_to_canonical_bun_entrypoint_plan() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    write_official_llxprt_wrapper(&directory, true, true);
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );

    let executable = resolver
        .resolve(AgentKind::Llxprt)
        .unwrap_or_else(|error| panic!("official wrapper should resolve: {error}"));
    let plan = executable
        .script_launch_plan()
        .unwrap_or_else(|| panic!("official wrapper must produce a canonical script plan"));
    // Issue #432: on Windows the stored runtime/entrypoint paths are the
    // canonicalize output with the `\\?\` verbatim prefix stripped (Node's
    // module loader mishandles the verbatim prefix). Build the expected
    // values through the same helper so the assertion tracks the production
    // path rather than re-implementing the strip.
    let expected_bun = expected_canonical(directory.path().join(LLXPRT_BUN_REL));
    let expected_entry = expected_canonical(directory.path().join(LLXPRT_ENTRYPOINT_REL));
    assert_eq!(executable.path(), directory.path().join("llxprt.cmd"));
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert_eq!(plan.runtime(), expected_bun);
    assert_eq!(plan.entrypoint(), expected_entry);
}

#[test]
fn windows_official_llxprt_wrapper_missing_bun_fails_safely() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    write_official_llxprt_wrapper(&directory, false, true);
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );

    let error = resolver
        .resolve(AgentKind::Llxprt)
        .err()
        .unwrap_or_else(|| panic!("incomplete official layout must fail"));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("LLxprt"));
    assert!(diagnostic.contains("reinstall"));
    assert!(!diagnostic.contains("prompt"));
}

#[test]
fn windows_official_llxprt_wrapper_missing_entrypoint_fails_safely() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    write_official_llxprt_wrapper(&directory, true, false);
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );

    let error = resolver
        .resolve(AgentKind::Llxprt)
        .err()
        .unwrap_or_else(|| panic!("incomplete official layout must fail"));
    assert!(error.to_string().contains("reinstall"));
}

#[test]
fn windows_unmarked_llxprt_cmd_retains_command_script_behavior() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    write_candidate(&directory, "llxprt.cmd");
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );

    let executable = resolver
        .resolve(AgentKind::Llxprt)
        .unwrap_or_else(|error| panic!("unmarked wrapper should resolve: {error}"));
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert!(
        executable.script_launch_plan().is_none(),
        "unmarked wrapper must not produce a canonical script plan"
    );
}

#[test]
fn windows_oversized_marked_llxprt_wrapper_is_not_treated_as_official() {
    let directory =
        TempDir::new().unwrap_or_else(|error| panic!("create oversized wrapper dir: {error}"));
    let wrapper = directory.path().join("llxprt.cmd");
    let mut body = format!("@echo off\r\nrem {LLXPRT_WRAPPER_MARKER}\r\n").into_bytes();
    body.resize(8 * 1_024 + 1, b'x');
    std::fs::write(&wrapper, body)
        .unwrap_or_else(|error| panic!("write oversized wrapper: {error}"));

    let executable = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    )
    .resolve(AgentKind::Llxprt)
    .unwrap_or_else(|error| panic!("oversized wrapper should remain launchable: {error}"));

    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert!(executable.script_launch_plan().is_none());
}

// Issue #467 Slice 3 (AC6): the private pane host must establish Windows Job
// Object containment before spawning the worker, and any failure to do so must
// surface as a typed refusal that names containment so the host never starts a
// descendant tree it cannot reliably reap.
#[test]
fn agent_launcher_error_names_windows_containment_refusal() {
    use super::agent_launcher::AgentLauncherError;

    #[cfg(windows)]
    {
        let message = AgentLauncherError::ContainmentUnavailable.to_string();
        assert!(
            message.contains("containment"),
            "containment refusal must name the failing concern: got {message}"
        );
        assert!(
            message.contains("job object"),
            "containment refusal must name the job object: got {message}"
        );
        assert!(
            !message.contains("0x"),
            "containment refusal must not leak raw handle values: got {message}"
        );
    }

    #[cfg(not(windows))]
    {
        // Unix keeps the launch path exactly as before: containment is absent
        // from both the error surface and the spawn path.
        let variants = [
            AgentLauncherError::InvalidPlan,
            AgentLauncherError::PlanSerializationFailed,
            AgentLauncherError::PlanCreateFailed,
            AgentLauncherError::PlanWriteFailed,
            AgentLauncherError::PlanReadFailed,
            AgentLauncherError::InvalidPlanPayload,
            AgentLauncherError::CleanupFailed,
            AgentLauncherError::LaunchFailed,
        ];
        for variant in variants {
            assert!(
                !variant.to_string().contains("containment"),
                "Unix launch errors must not mention containment: {variant}"
            );
        }
    }
}
