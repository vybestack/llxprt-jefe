//! Agent runtime detection tests moved out of the lib target (issue #307).

use jefe::agent_detection::detect_agent_kinds;
use jefe::domain::AgentKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn temp_dir_with_binaries(binaries: &[&str]) -> PathBuf {
    let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "jefe-agent-detect-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create temp dir: {error}"));
    for binary in binaries {
        let filename = if cfg!(windows) {
            format!("{binary}.exe")
        } else {
            (*binary).to_owned()
        };
        let path = dir.join(filename);
        std::fs::write(&path, b"#!/bin/sh\n")
            .unwrap_or_else(|error| panic!("write binary: {error}"));
        make_executable(&path);
    }
    dir
}

#[test]
fn detect_neither_installed() {
    let dir = temp_dir_with_binaries(&[]);
    let detected = detect_agent_kinds(std::slice::from_ref(&dir));
    assert!(detected.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_only_llxprt() {
    let dir = temp_dir_with_binaries(&["llxprt"]);
    let detected = detect_agent_kinds(std::slice::from_ref(&dir));
    assert_eq!(detected, vec![AgentKind::Llxprt]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_only_code_puppy() {
    let dir = temp_dir_with_binaries(&["code-puppy"]);
    let detected = detect_agent_kinds(std::slice::from_ref(&dir));
    assert_eq!(detected, vec![AgentKind::CodePuppy]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_both_in_canonical_order() {
    let dir_cp = temp_dir_with_binaries(&["code-puppy"]);
    let dir_ll = temp_dir_with_binaries(&["llxprt"]);
    let detected = detect_agent_kinds(&[dir_cp.clone(), dir_ll.clone()]);
    assert_eq!(detected, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
    let _ = std::fs::remove_dir_all(&dir_cp);
    let _ = std::fs::remove_dir_all(&dir_ll);
}

#[test]
fn detect_both_in_same_dir() {
    let dir = temp_dir_with_binaries(&["llxprt", "code-puppy"]);
    let detected = detect_agent_kinds(std::slice::from_ref(&dir));
    assert_eq!(detected, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn detect_requires_executable_permission() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "jefe-agent-detect-noexec-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let path = dir.join("llxprt");
    std::fs::write(&path, b"#!/bin/sh\n")
        .unwrap_or_else(|error| panic!("write binary: {error}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .unwrap_or_else(|error| panic!("set non-executable permissions: {error}"));

    let detected = detect_agent_kinds(std::slice::from_ref(&dir));
    assert!(
        detected.is_empty(),
        "non-executable file must not be detected"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_ignores_nonexistent_dirs() {
    let fake_dir = PathBuf::from("/this/path/does/not/exist/jefe-test");
    let detected = detect_agent_kinds(&[fake_dir]);
    assert!(detected.is_empty());
}
