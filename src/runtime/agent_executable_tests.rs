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
    let Some(plan) = executable.npm_launch_plan() else {
        panic!("npm.cmd must retain a canonical direct Node.js plan");
    };

    assert_eq!(executable.path(), expected);
    assert_eq!(executable.wrapper_kind(), AgentWrapperKind::CommandScript);
    assert_eq!(executable.target(), AgentExecutableTarget::Npm);
    assert_eq!(
        plan.node(),
        std::fs::canonicalize(node).unwrap_or_else(|error| panic!("node: {error}"))
    );
    assert_eq!(
        plan.cli(),
        std::fs::canonicalize(cli).unwrap_or_else(|error| panic!("cli: {error}"))
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
