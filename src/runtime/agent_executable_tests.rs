//! Behavioral contracts for native agent executable resolution and launch planning.

use std::ffi::OsString;
use std::path::PathBuf;

use tempfile::TempDir;

use super::agent_executable::{
    AgentExecutableError, AgentExecutablePlatform, AgentExecutableResolver, AgentWrapperKind,
    NpmDirectInvocation, ResolvedAgentExecutable,
};
use crate::domain::AgentKind;

fn write_candidate(directory: &TempDir, name: &str) -> PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, b"fixture")
        .unwrap_or_else(|error| panic!("write executable fixture: {error}"));
    path
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

// ── resolve_named (npm) tests (issue #269) ──────────────────────────────────

#[cfg(unix)]
#[test]
fn resolve_named_npm_unix_finds_executable_via_execute_permission() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let npm_path = directory.path().join("npm");
    std::fs::write(&npm_path, b"#!/bin/sh\n").unwrap_or_else(|error| panic!("write npm: {error}"));
    std::fs::set_permissions(&npm_path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm: {error}"));

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![directory.path().to_path_buf()],
        None,
    );
    let executable = policy
        .resolve_named("npm")
        .unwrap_or_else(|error| panic!("npm should resolve: {error}"));
    assert_eq!(executable.path(), &npm_path);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::Direct);
}

#[cfg(unix)]
#[test]
fn resolve_named_npm_unix_rejects_non_executable_file() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let npm_path = directory.path().join("npm");
    std::fs::write(&npm_path, b"data").unwrap_or_else(|error| panic!("write npm: {error}"));
    // No execute permission.

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![directory.path().to_path_buf()],
        None,
    );
    let Err(error) = policy.resolve_named("npm") else {
        panic!("non-executable npm must not resolve");
    };
    assert!(matches!(error, AgentExecutableError::NamedNotFound { .. }));
    assert!(error.to_string().contains("npm"));
}

#[test]
fn resolve_named_npm_windows_finds_npm_cmd_via_pathext() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let npm_cmd = create_standard_npm_layout(&directory);

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
    );
    let executable = policy
        .resolve_named("npm")
        .unwrap_or_else(|error| panic!("npm.cmd with standard layout should resolve: {error}"));
    assert_eq!(executable.path(), npm_cmd);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    // npm_direct must be populated so the launcher bypasses cmd.exe.
    assert!(
        executable.npm_direct().is_some(),
        "standard npm.cmd layout must derive npm_direct"
    );
}

/// Cross-platform: the Windows resolver must reject npm.cmd with a
/// non-standard layout (no node.exe / no npm-cli.js) with
/// `NpmWrapperResolutionFailed` — never silently fall back to cmd.exe.
#[test]
fn resolve_named_npm_windows_rejects_nonstandard_npm_cmd_layout() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    // npm.cmd exists but NO node.exe and NO npm-cli.js.
    write_candidate(&directory, "npm.cmd");

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
    );
    let result = policy.resolve_named("npm");
    let Err(error) = result else {
        panic!("non-standard npm.cmd layout must be rejected, not silently fall back to cmd.exe");
    };
    assert!(
        matches!(error, AgentExecutableError::NpmWrapperResolutionFailed(_)),
        "expected NpmWrapperResolutionFailed, got {error:?}"
    );
}

/// Cross-platform: a non-npm .cmd (e.g. `yarn.cmd`) must still resolve via
/// the Windows resolver with the CommandScript wrapper — generic command
/// scripts remain supported for unrelated callers.
#[test]
fn resolve_named_windows_non_npm_cmd_succeeds_without_npm_direct() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let yarn_cmd = write_candidate(&directory, "yarn.cmd");

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".CMD")),
    );
    let executable = policy
        .resolve_named("yarn")
        .unwrap_or_else(|error| panic!("non-npm .cmd should resolve: {error}"));
    assert_eq!(executable.path(), yarn_cmd);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert!(
        executable.npm_direct().is_none(),
        "non-npm .cmd must not get npm_direct"
    );
}

#[test]
fn resolve_named_npm_windows_finds_npm_exe_when_both_exist() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let npm_exe = write_candidate(&directory, "npm.exe");
    write_candidate(&directory, "npm.cmd");

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        Some(OsString::from(".EXE;.CMD")),
    );
    let executable = policy
        .resolve_named("npm")
        .unwrap_or_else(|error| panic!("npm should resolve: {error}"));
    assert_eq!(executable.path(), npm_exe);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::Direct);
}

#[test]
fn resolve_named_npm_windows_rejects_npm_bare_when_pathext_omits_extensions() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    write_candidate(&directory, "npm");

    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![directory.path().to_path_buf()],
        None, // falls back to default PATHEXT
    );
    let result = policy.resolve_named("npm");
    // Default PATHEXT doesn't include bare names, so npm without an extension
    // should not resolve.
    assert!(
        result.is_err(),
        "bare npm without extension must not resolve under default PATHEXT"
    );
}

#[test]
fn resolve_named_reports_safe_error_when_not_found() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let policy = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![directory.path().to_path_buf()],
        None,
    );
    let Err(error) = policy.resolve_named("nonexistent-tool") else {
        panic!("missing tool must error");
    };
    assert!(matches!(error, AgentExecutableError::NamedNotFound { .. }));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("nonexistent-tool"));
}

// ── ResolvedAgentExecutable::from_path tests ────────────────────────────────

#[cfg(unix)]
#[test]
fn from_path_unix_returns_direct_wrapper() {
    let resolved = ResolvedAgentExecutable::from_path(std::path::Path::new("/usr/local/bin/npm"))
        .unwrap_or_else(|e| panic!("from_path should succeed on Unix: {e}"));
    assert_eq!(resolved.path(), std::path::Path::new("/usr/local/bin/npm"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::Direct);
}

#[cfg(windows)]
#[test]
fn from_path_windows_classifies_exe_as_direct() {
    let resolved = ResolvedAgentExecutable::from_path(std::path::Path::new("C:\\node\\npm.exe"))
        .unwrap_or_else(|e| panic!("non-npm .exe should resolve: {e}"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::Direct);
}

#[cfg(windows)]
#[test]
fn from_path_windows_classifies_ps1_as_powershell_script() {
    let resolved = ResolvedAgentExecutable::from_path(std::path::Path::new("C:\\node\\npm.ps1"))
        .unwrap_or_else(|e| panic!("non-npm .ps1 should resolve: {e}"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::PowerShellScript);
}

#[cfg(windows)]
#[test]
fn from_path_windows_unknown_extension_defaults_to_direct() {
    let resolved = ResolvedAgentExecutable::from_path(std::path::Path::new("C:\\node\\npm"))
        .unwrap_or_else(|e| panic!("unknown extension should resolve: {e}"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::Direct);
}

// ── NpmDirectInvocation derivation tests (Windows npm.cmd safety) ──────────
//
// When the resolved npm is .cmd/.bat on Windows, the agent launcher must
// derive a direct node.exe + npm-cli.js invocation so the selector and all
// npm arguments remain structural argv and never pass through cmd.exe.

/// Create a standard npm installation layout in a temp directory and return
/// the npm.cmd path.
fn create_standard_npm_layout(directory: &TempDir) -> PathBuf {
    let npm_cmd = directory.path().join("npm.cmd");
    std::fs::write(&npm_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npm.cmd: {e}"));
    let node_exe = directory.path().join("node.exe");
    std::fs::write(&node_exe, b"binary").unwrap_or_else(|e| panic!("write node.exe: {e}"));
    let npm_dir = directory
        .path()
        .join("node_modules")
        .join("npm")
        .join("bin");
    std::fs::create_dir_all(&npm_dir).unwrap_or_else(|e| panic!("mkdir bin: {e}"));
    std::fs::write(npm_dir.join("npm-cli.js"), b"// cli")
        .unwrap_or_else(|e| panic!("write npm-cli.js: {e}"));
    npm_cmd
}

#[test]
fn npm_direct_invocation_from_wrapper_resolves_node_and_cli() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = create_standard_npm_layout(&directory);

    let invocation = NpmDirectInvocation::from_wrapper(&npm_cmd)
        .unwrap_or_else(|e| panic!("npm direct should resolve: {e}"));
    assert_eq!(
        invocation.node_executable(),
        directory.path().join("node.exe")
    );
    assert_eq!(
        invocation.cli_script(),
        directory
            .path()
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js")
    );
}

#[test]
fn npm_direct_invocation_from_wrapper_fails_when_node_exe_missing() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = directory.path().join("npm.cmd");
    std::fs::write(&npm_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npm.cmd: {e}"));
    // node.exe is intentionally absent.
    let npm_dir = directory
        .path()
        .join("node_modules")
        .join("npm")
        .join("bin");
    std::fs::create_dir_all(&npm_dir).unwrap_or_else(|e| panic!("mkdir bin: {e}"));
    std::fs::write(npm_dir.join("npm-cli.js"), b"// cli")
        .unwrap_or_else(|e| panic!("write cli: {e}"));

    let result = NpmDirectInvocation::from_wrapper(&npm_cmd);
    let error = match result {
        Err(error) => error,
        Ok(value) => panic!("missing node.exe must fail derivation, got {value:?}"),
    };
    assert!(matches!(
        error,
        AgentExecutableError::NpmWrapperResolutionFailed(_)
    ));
    assert!(error.to_string().contains("node.exe"));
}

#[test]
fn npm_direct_invocation_from_wrapper_fails_when_cli_script_missing() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = directory.path().join("npm.cmd");
    std::fs::write(&npm_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npm.cmd: {e}"));
    let node_exe = directory.path().join("node.exe");
    std::fs::write(&node_exe, b"binary").unwrap_or_else(|e| panic!("write node.exe: {e}"));
    // npm-cli.js is intentionally absent (node_modules/npm/bin not created).

    let result = NpmDirectInvocation::from_wrapper(&npm_cmd);
    let error = match result {
        Err(error) => error,
        Ok(value) => panic!("missing cli script must fail derivation, got {value:?}"),
    };
    assert!(error.to_string().contains("npm-cli.js"));
}

#[test]
fn npm_direct_invocation_from_wrapper_rejects_unsupported_wrapper_name() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let weird_cmd = directory.path().join("yarn.cmd");
    std::fs::write(&weird_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write cmd: {e}"));

    let result = NpmDirectInvocation::from_wrapper(&weird_cmd);
    let error = match result {
        Err(error) => error,
        Ok(value) => panic!("non-npm wrapper must fail derivation, got {value:?}"),
    };
    assert!(error.to_string().contains("yarn"));
}

#[test]
fn npm_direct_invocation_from_wrapper_resolves_npx_cli() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npx_cmd = directory.path().join("npx.cmd");
    std::fs::write(&npx_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npx.cmd: {e}"));
    let node_exe = directory.path().join("node.exe");
    std::fs::write(&node_exe, b"binary").unwrap_or_else(|e| panic!("write node.exe: {e}"));
    let npm_dir = directory
        .path()
        .join("node_modules")
        .join("npm")
        .join("bin");
    std::fs::create_dir_all(&npm_dir).unwrap_or_else(|e| panic!("mkdir bin: {e}"));
    std::fs::write(npm_dir.join("npx-cli.js"), b"// cli")
        .unwrap_or_else(|e| panic!("write npx-cli.js: {e}"));

    let invocation = NpmDirectInvocation::from_wrapper(&npx_cmd)
        .unwrap_or_else(|e| panic!("npx direct should resolve: {e}"));
    let cli_file = invocation
        .cli_script()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("cli_script must have a file name"));
    assert_eq!(cli_file, "npx-cli.js");
}

#[cfg(windows)]
#[test]
fn from_path_windows_npm_cmd_derives_npm_direct_invocation() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = create_standard_npm_layout(&directory);

    let resolved = ResolvedAgentExecutable::from_path(&npm_cmd)
        .unwrap_or_else(|e| panic!("standard npm layout should resolve: {e}"));
    // The wrapper kind stays CommandScript, but npm_direct is populated.
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::CommandScript);
    let npm_direct = resolved
        .npm_direct()
        .expect("npm.cmd with standard layout must derive npm_direct");
    assert_eq!(
        npm_direct.node_executable(),
        directory.path().join("node.exe")
    );
}

/// Non-standard npm.cmd layout MUST fail with `NpmWrapperResolutionFailed` —
/// never silently fall back to cmd.exe (issue #269).
#[cfg(windows)]
#[test]
fn from_path_windows_npm_cmd_without_standard_layout_fails() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let npm_cmd = directory.path().join("npm.cmd");
    std::fs::write(&npm_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write npm.cmd: {e}"));
    // No node.exe, no node_modules/npm/bin/npm-cli.js.

    let result = ResolvedAgentExecutable::from_path(&npm_cmd);
    let Err(error) = result else {
        panic!("non-standard npm.cmd layout must NOT silently fall back to cmd.exe");
    };
    assert!(
        matches!(error, AgentExecutableError::NpmWrapperResolutionFailed(_)),
        "expected NpmWrapperResolutionFailed, got {error:?}"
    );
}

/// Non-npm .cmd files (e.g. `something.cmd`) must still resolve with the
/// `CommandScript` wrapper and no `npm_direct` — generic command scripts
/// remain supported for unrelated callers.
#[cfg(windows)]
#[test]
fn from_path_windows_non_npm_cmd_succeeds_without_npm_direct() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let other_cmd = directory.path().join("something.cmd");
    std::fs::write(&other_cmd, b"@echo off\r\n").unwrap_or_else(|e| panic!("write cmd: {e}"));

    let resolved = ResolvedAgentExecutable::from_path(&other_cmd)
        .unwrap_or_else(|e| panic!("non-npm .cmd should resolve: {e}"));
    assert_eq!(resolved.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert!(
        resolved.npm_direct().is_none(),
        "non-npm .cmd must not get npm_direct"
    );
}
