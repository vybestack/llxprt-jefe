//! Unit tests for the captured PATH snapshot and platform launchable policy.

use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(unix)]
use tempfile::TempDir;

use super::PathSnapshot;
use crate::runtime::{AgentExecutablePlatform, AgentWrapperKind};

#[cfg(unix)]
fn make_executable(dir: &TempDir, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.path().join(name);
    std::fs::write(&path, b"#!/bin/sh\n").unwrap_or_else(|error| panic!("write fixture: {error}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod fixture: {error}"));
    path
}

#[cfg(unix)]
#[test]
fn unix_resolves_first_launchable_in_path_order() {
    let Ok(first) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let Ok(second) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let expected = make_executable(&first, "agent");
    make_executable(&second, "agent");
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![first.path().to_path_buf(), second.path().to_path_buf()],
        None,
    );
    let Some((resolved, wrapper)) = snapshot.resolve_binary("agent") else {
        panic!("first launchable resolves");
    };
    assert_eq!(resolved, expected);
    assert_eq!(wrapper, AgentWrapperKind::Direct);
}

#[cfg(unix)]
#[test]
fn unix_returns_none_when_no_launchable_present() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    // Present but not executable: must not resolve.
    let path = dir.path().join("agent");
    std::fs::write(&path, b"nope").unwrap_or_else(|error| panic!("write fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![dir.path().to_path_buf()],
        None,
    );
    assert!(
        snapshot.resolve_binary("agent").is_none(),
        "non-executable file must not resolve"
    );
}

#[cfg(unix)]
#[test]
fn unix_returns_none_when_name_absent_from_all_dirs() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![dir.path().to_path_buf()],
        None,
    );
    assert!(snapshot.resolve_binary("absent").is_none());
}

#[cfg(unix)]
#[test]
fn unix_resolves_symlink_to_target() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let target = make_executable(&dir, "real-binary");
    let link = dir.path().join("agent");
    std::os::unix::fs::symlink(&target, &link)
        .unwrap_or_else(|error| panic!("symlink fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![dir.path().to_path_buf()],
        None,
    );
    let Some((resolved, _wrapper)) = snapshot.resolve_binary("agent") else {
        panic!("symlink resolves when target is executable");
    };
    // Resolution returns the symlink path; canonicalization happens in the
    // fingerprint stage. The key contract here is that the symlink itself is
    // launchable because its target is.
    assert_eq!(resolved, link);
}

#[test]
fn windows_resolves_pathext_order() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let exe_path = dir.path().join("agent.exe");
    std::fs::write(&exe_path, b"exe").unwrap_or_else(|error| panic!("write fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Windows,
        vec![dir.path().to_path_buf()],
        Some(OsString::from(".COM;.EXE;.BAT;.CMD")),
    );
    let Some((resolved, wrapper)) = snapshot.resolve_binary("agent") else {
        panic!("first matching extension resolves");
    };
    assert_eq!(resolved, exe_path);
    assert_eq!(wrapper, AgentWrapperKind::Direct);
}

#[test]
fn windows_classifies_command_script_wrapper() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let cmd_path = dir.path().join("agent.cmd");
    std::fs::write(&cmd_path, b"@echo off")
        .unwrap_or_else(|error| panic!("write fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Windows,
        vec![dir.path().to_path_buf()],
        Some(OsString::from(".EXE;.CMD")),
    );
    let Some((resolved, wrapper)) = snapshot.resolve_binary("agent") else {
        panic!("cmd resolves under pathext");
    };
    assert_eq!(resolved, cmd_path);
    assert_eq!(wrapper, AgentWrapperKind::CommandScript);
}

#[test]
fn windows_returns_none_when_no_pathext_match() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    std::fs::write(dir.path().join("agent.js"), b"nope")
        .unwrap_or_else(|error| panic!("write fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Windows,
        vec![dir.path().to_path_buf()],
        Some(OsString::from(".EXE;.CMD")),
    );
    assert!(snapshot.resolve_binary("agent").is_none());
}

#[cfg(unix)]
#[test]
fn resolve_repository_local_joins_to_root_and_requires_executable() {
    use std::os::unix::fs::PermissionsExt;
    let Ok(repo) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let bin_dir = repo.path().join(".llxprt/bin");
    std::fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    let exe = bin_dir.join("llxprt");
    std::fs::write(&exe, b"#!/bin/sh\n").unwrap_or_else(|error| panic!("write fixture: {error}"));
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod fixture: {error}"));

    let snapshot = PathSnapshot::for_platform(AgentExecutablePlatform::Unix, vec![], None);
    let Some((resolved, wrapper)) = super::resolve_repository_local(
        &snapshot,
        repo.path(),
        std::path::Path::new(".llxprt/bin/llxprt"),
    ) else {
        panic!("repository-local launchable resolves");
    };
    assert_eq!(resolved, exe);
    assert_eq!(wrapper, AgentWrapperKind::Direct);
}

#[cfg(unix)]
#[test]
fn resolve_repository_local_none_when_not_executable() {
    let Ok(repo) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let bin_dir = repo.path().join(".llxprt/bin");
    std::fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    std::fs::write(bin_dir.join("llxprt"), b"nope")
        .unwrap_or_else(|error| panic!("write fixture: {error}"));
    let snapshot = PathSnapshot::for_platform(AgentExecutablePlatform::Unix, vec![], None);
    assert!(
        super::resolve_repository_local(
            &snapshot,
            repo.path(),
            std::path::Path::new(".llxprt/bin/llxprt")
        )
        .is_none()
    );
}

#[test]
fn directories_borrowed_in_order() {
    let dirs = vec![
        PathBuf::from("/first"),
        PathBuf::from("/second"),
        PathBuf::from("/third"),
    ];
    let snapshot = PathSnapshot::for_platform(AgentExecutablePlatform::Unix, dirs.clone(), None);
    assert_eq!(snapshot.directories(), dirs.as_slice());
    assert_eq!(snapshot.platform(), AgentExecutablePlatform::Unix);
}
