//! Behavioral tests for workspace containment (issue #380, CW00-07).

use std::os::unix::fs::PermissionsExt;

use super::super::contract::{DirSpec, FileContent, FileSpec, RelPath, WorkspaceSpec};
use super::super::error::HarCode;
use super::{ENV_DIRS, Workspace};

fn empty_spec() -> WorkspaceSpec {
    WorkspaceSpec {
        dirs: vec![],
        files: vec![],
        env: vec![],
    }
}

fn rel(path: &str) -> RelPath {
    super::super::validate::validate_rel_path("test path", path)
        .unwrap_or_else(|err| panic!("valid test path: {err}"))
}

struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn create(spec: &WorkspaceSpec) -> (Workspace, Cleanup) {
    let workspace =
        Workspace::create(spec).unwrap_or_else(|err| panic!("workspace should create: {err}"));
    let cleanup = Cleanup(workspace.root().to_path_buf());
    (workspace, cleanup)
}

#[test]
fn creates_mode_700_root_with_env_dirs() {
    let (workspace, _cleanup) = create(&empty_spec());
    let metadata =
        std::fs::metadata(workspace.root()).unwrap_or_else(|err| panic!("root should stat: {err}"));
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    for dir in ENV_DIRS {
        assert!(
            workspace.root().join(dir).is_dir(),
            "env dir {dir} should exist"
        );
    }
}

#[test]
fn materializes_dirs_and_files_with_declared_modes() {
    let spec = WorkspaceSpec {
        dirs: vec![DirSpec {
            path: rel("work"),
            mode: 0o755,
        }],
        files: vec![FileSpec {
            path: rel("work/tool"),
            content: FileContent::Utf8("#!x".to_string()),
            mode: 0o755,
        }],
        env: vec![],
    };
    let (workspace, _cleanup) = create(&spec);
    let dir_mode = std::fs::metadata(workspace.root().join("work"))
        .unwrap_or_else(|err| panic!("dir should stat: {err}"))
        .permissions()
        .mode();
    assert_eq!(dir_mode & 0o777, 0o755);
    let file_path = workspace.root().join("work/tool");
    let file_mode = std::fs::metadata(&file_path)
        .unwrap_or_else(|err| panic!("file should stat: {err}"))
        .permissions()
        .mode();
    assert_eq!(file_mode & 0o777, 0o755);
    let content =
        std::fs::read_to_string(&file_path).unwrap_or_else(|err| panic!("file should read: {err}"));
    assert_eq!(content, "#!x");
}

#[test]
fn write_read_remove_and_exists_round_trip() {
    let spec = WorkspaceSpec {
        dirs: vec![DirSpec {
            path: rel("data"),
            mode: 0o700,
        }],
        files: vec![],
        env: vec![],
    };
    let (mut workspace, _cleanup) = create(&spec);
    workspace
        .write_file(&FileSpec {
            path: rel("data/f.bin"),
            content: FileContent::Base64(vec![0, 159, 146, 150]),
            mode: 0o600,
        })
        .unwrap_or_else(|err| panic!("write should pass: {err}"));
    let bytes = workspace
        .read_file(&rel("data/f.bin"))
        .unwrap_or_else(|err| panic!("read should pass: {err}"));
    assert_eq!(bytes, vec![0, 159, 146, 150]);
    assert!(
        workspace
            .exists(&rel("data/f.bin"))
            .unwrap_or_else(|err| panic!("exists should pass: {err}"))
    );
    workspace
        .remove(&rel("data/f.bin"))
        .unwrap_or_else(|err| panic!("remove should pass: {err}"));
    assert!(
        !workspace
            .exists(&rel("data/f.bin"))
            .unwrap_or_else(|err| panic!("exists should pass: {err}"))
    );
}

#[test]
fn symlink_target_write_is_containment_error() {
    let (mut workspace, _cleanup) = create(&empty_spec());
    let outside = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, "safe").unwrap_or_else(|err| panic!("seed victim: {err}"));
    std::os::unix::fs::symlink(&victim, workspace.root().join("evil"))
        .unwrap_or_else(|err| panic!("symlink: {err}"));
    let err = workspace
        .write_file(&FileSpec {
            path: rel("evil"),
            content: FileContent::Utf8("attack".to_string()),
            mode: 0o600,
        })
        .err()
        .unwrap_or_else(|| panic!("write through symlink must fail"));
    assert_eq!(err.code, HarCode::E004);
    let content =
        std::fs::read_to_string(&victim).unwrap_or_else(|err| panic!("victim read: {err}"));
    assert_eq!(content, "safe", "victim file must be untouched");
}

#[test]
fn symlink_ancestor_swap_is_rejected_before_access() {
    let spec = WorkspaceSpec {
        dirs: vec![DirSpec {
            path: rel("safe"),
            mode: 0o700,
        }],
        files: vec![],
        env: vec![],
    };
    let (mut workspace, _cleanup) = create(&spec);
    // Swap the known ancestor for a symlink pointing outside the workspace.
    let outside = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir: {err}"));
    std::fs::remove_dir(workspace.root().join("safe"))
        .unwrap_or_else(|err| panic!("remove dir: {err}"));
    std::os::unix::fs::symlink(outside.path(), workspace.root().join("safe"))
        .unwrap_or_else(|err| panic!("symlink swap: {err}"));
    let err = workspace
        .write_file(&FileSpec {
            path: rel("safe/leak.txt"),
            content: FileContent::Utf8("attack".to_string()),
            mode: 0o600,
        })
        .err()
        .unwrap_or_else(|| panic!("write through swapped ancestor must fail"));
    assert_eq!(err.code, HarCode::E004);
    assert!(
        !outside.path().join("leak.txt").exists(),
        "no file may appear outside the workspace"
    );
}

#[test]
fn replaced_ancestor_directory_identity_is_rejected() {
    let spec = WorkspaceSpec {
        dirs: vec![DirSpec {
            path: rel("sub"),
            mode: 0o700,
        }],
        files: vec![],
        env: vec![],
    };
    let (mut workspace, _cleanup) = create(&spec);
    // Replace the directory with another real directory (same path, new inode).
    std::fs::remove_dir(workspace.root().join("sub"))
        .unwrap_or_else(|err| panic!("remove dir: {err}"));
    std::fs::create_dir(workspace.root().join("sub"))
        .unwrap_or_else(|err| panic!("recreate dir: {err}"));
    let err = workspace
        .write_file(&FileSpec {
            path: rel("sub/file.txt"),
            content: FileContent::Utf8("x".to_string()),
            mode: 0o600,
        })
        .err()
        .unwrap_or_else(|| panic!("write below replaced ancestor must fail"));
    assert_eq!(err.code, HarCode::E004);
}

#[test]
fn remove_forgets_recorded_identities_below_target() {
    let spec = WorkspaceSpec {
        dirs: vec![DirSpec {
            path: rel("tree"),
            mode: 0o700,
        }],
        files: vec![],
        env: vec![],
    };
    let (mut workspace, _cleanup) = create(&spec);
    workspace
        .remove(&rel("tree"))
        .unwrap_or_else(|err| panic!("remove should pass: {err}"));
    // Recreate through the workspace API; the fresh identity must be accepted.
    workspace
        .mkdir(&DirSpec {
            path: rel("tree"),
            mode: 0o700,
        })
        .unwrap_or_else(|err| panic!("mkdir should pass: {err}"));
    workspace
        .write_file(&FileSpec {
            path: rel("tree/ok.txt"),
            content: FileContent::Utf8("ok".to_string()),
            mode: 0o600,
        })
        .unwrap_or_else(|err| panic!("write should pass: {err}"));
}

#[test]
fn distinct_workspaces_do_not_collide() {
    let (first, _c1) = create(&empty_spec());
    let (second, _c2) = create(&empty_spec());
    assert_ne!(first.root(), second.root());
}
