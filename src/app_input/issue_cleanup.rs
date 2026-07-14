//! Ownership-scoped local dirty-worktree cleanup.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

use super::{git_capture, git_require_success, require_success};

pub(in crate::app_input) fn discard_workdir_changes(work_dir: &Path) -> Result<(), String> {
    let output = git_capture(
        work_dir,
        [
            "-c",
            "core.quotepath=false",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;
    require_success(&output, "ls-files --others --exclude-standard -z")?;
    let untracked = parse_changed_paths(&output.stdout)?;
    for relative in &untracked {
        remove_untracked_path(work_dir, relative)?;
    }
    remove_empty_untracked_parents(work_dir, &untracked)?;
    restore_tracked_paths(work_dir)
}

fn restore_tracked_paths(work_dir: &Path) -> Result<(), String> {
    let output = git_capture(
        work_dir,
        [
            "-c",
            "core.quotepath=false",
            "diff",
            "--name-only",
            "-z",
            "HEAD",
        ],
    )?;
    require_success(&output, "diff --name-only -z HEAD")?;
    for relative in parse_changed_paths(&output.stdout)? {
        git_require_success(
            work_dir,
            [
                std::ffi::OsStr::new("restore"),
                std::ffi::OsStr::new("--source=HEAD"),
                std::ffi::OsStr::new("--staged"),
                std::ffi::OsStr::new("--worktree"),
                std::ffi::OsStr::new("--"),
                relative.as_os_str(),
            ],
        )?;
    }
    Ok(())
}

fn parse_changed_paths(stdout: &[u8]) -> Result<Vec<PathBuf>, String> {
    let mut paths = parse_paths(stdout)?;
    paths.retain(|path| !is_owned_path(path));
    Ok(paths)
}

fn is_owned_path(path: &Path) -> bool {
    path.components().next().is_some_and(|component| {
        let value = component.as_os_str();
        value == ".jefe" || value == ".llxprt"
    })
}

fn parse_paths(stdout: &[u8]) -> Result<Vec<PathBuf>, String> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            let path = path_from_git_bytes(raw)?;
            let safe = !path.is_absolute()
                && path
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)));
            safe.then_some(path.clone()).ok_or_else(|| {
                format!("git returned an unsafe path: {}", path.display())
            })
        })
        .collect()
}

#[cfg(unix)]
fn path_from_git_bytes(raw: &[u8]) -> Result<PathBuf, String> {
    Ok(PathBuf::from(OsString::from_vec(raw.to_vec())))
}

#[cfg(not(unix))]
fn path_from_git_bytes(raw: &[u8]) -> Result<PathBuf, String> {
    std::str::from_utf8(raw)
        .map(PathBuf::from)
        .map_err(|error| format!("git returned a non-UTF-8 path: {error}"))
}

fn remove_untracked_path(work_dir: &Path, relative: &Path) -> Result<(), String> {
    let path = work_dir.join(relative);
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "Failed to inspect untracked path {} before cleanup: {error}. Close programs using the file and retry.",
            path.display()
        )
    })?;
    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir(&path)
    } else {
        std::fs::remove_file(&path)
    };
    result.map_err(|error| {
        format!(
            "Failed to remove untracked path {}: {error}. Close programs using the file and retry.",
            path.display()
        )
    })
}

fn remove_empty_untracked_parents(work_dir: &Path, paths: &[PathBuf]) -> Result<(), String> {
    for relative in paths {
        let mut parent = relative.parent();
        while let Some(candidate) = parent.filter(|value| !value.as_os_str().is_empty()) {
            let directory = work_dir.join(candidate);
            match std::fs::remove_dir(&directory) {
                Ok(()) => parent = candidate.parent(),
                Err(_error) if directory_has_entries(&directory) => break,
                Err(error) => {
                    return Err(format!(
                        "Failed to remove empty untracked directory {}: {error}",
                        directory.display()
                    ));
                }
            }
        }
    }
    Ok(())
}

fn directory_has_entries(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some())
}
