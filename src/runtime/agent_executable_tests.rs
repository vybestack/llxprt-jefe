//! Behavioral contracts for native agent executable resolution and launch planning.

use std::ffi::OsString;
use std::path::PathBuf;

use tempfile::TempDir;

use super::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver, AgentWrapperKind};
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
