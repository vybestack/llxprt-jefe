//! Explicit local executable resolution and typed subprocess construction.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Supported local command-line tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalTool {
    /// Git command-line client.
    Git,
    /// GitHub command-line client.
    Gh,
}

impl LocalTool {
    fn name(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Gh => "gh",
        }
    }

    fn override_name(self) -> &'static str {
        match self {
            Self::Git => "JEFE_GIT_BIN",
            Self::Gh => "JEFE_GH_BIN",
        }
    }
}

/// Host executable-resolution policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolPlatform {
    Windows,
    Unix,
}

impl ToolPlatform {
    const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Failure to resolve a required local tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalToolError {
    /// The executable was not found on `PATH`.
    NotFound { tool: LocalTool },
}

impl fmt::Display for LocalToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { tool } => write!(
                formatter,
                "{} executable not found; install it or set {}",
                tool.name(),
                tool.override_name()
            ),
        }
    }
}

impl std::error::Error for LocalToolError {}

/// Resolve a local tool to an explicit executable path.
pub fn resolve(tool: LocalTool) -> Result<PathBuf, LocalToolError> {
    let override_path = std::env::var_os(tool.override_name()).map(PathBuf::from);
    let paths =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    resolve_in(
        tool,
        ToolPlatform::current(),
        &paths,
        std::env::var_os("PATHEXT"),
        override_path,
    )
}

/// Construct a command using an explicitly resolved executable.
pub fn command(tool: LocalTool) -> Result<Command, LocalToolError> {
    resolve(tool).map(Command::new)
}

fn resolve_in(
    tool: LocalTool,
    platform: ToolPlatform,
    paths: &[PathBuf],
    pathext: Option<OsString>,
    override_path: Option<PathBuf>,
) -> Result<PathBuf, LocalToolError> {
    if let Some(path) = override_path {
        return Ok(path);
    }
    let candidates = executable_names(tool.name(), platform, pathext.as_deref());
    for directory in paths {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if executable_file(&path, platform) {
                return Ok(path);
            }
        }
    }
    Err(LocalToolError::NotFound { tool })
}

fn executable_names(name: &str, platform: ToolPlatform, pathext: Option<&OsStr>) -> Vec<OsString> {
    if platform == ToolPlatform::Unix {
        return vec![OsString::from(name)];
    }
    let extensions = pathext
        .and_then(OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(WINDOWS_DEFAULT_PATHEXT);
    extensions
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{extension}"))
        .map(OsString::from)
        .collect()
}

fn executable_file(path: &Path, platform: ToolPlatform) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    if platform == ToolPlatform::Unix {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0);
    }
    let _ = platform;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn windows_resolves_pathext_in_directory_order_with_spaces_and_unicode() {
        let root = tempfile::Builder::new()
            .prefix("jefe tools Ω ")
            .tempdir()
            .unwrap_or_else(|error| panic!("create tool directory: {error}"));
        let second = root.path().join("second tools");
        std::fs::create_dir_all(&second)
            .unwrap_or_else(|error| panic!("create second tool directory: {error}"));
        let executable = second.join("git.EXE");
        std::fs::write(&executable, b"fixture")
            .unwrap_or_else(|error| panic!("write tool fixture: {error}"));

        let resolved = resolve_in(
            LocalTool::Git,
            ToolPlatform::Windows,
            &[root.path().to_path_buf(), second],
            Some(OsString::from(".CMD;.EXE")),
            None,
        );

        assert_eq!(resolved, Ok(executable));
    }

    #[test]
    fn explicit_override_is_preserved_as_a_path() {
        let override_path = std::path::PathBuf::from(r"C:\Program Files\Git Ω\bin\git.exe");
        let resolved = resolve_in(
            LocalTool::Git,
            ToolPlatform::Windows,
            &[],
            None,
            Some(override_path.clone()),
        );
        assert_eq!(resolved, Ok(override_path));
    }

    #[test]
    fn missing_tool_is_a_typed_error() {
        let result = resolve_in(LocalTool::Gh, ToolPlatform::Unix, &[], None, None);
        assert!(matches!(
            result,
            Err(LocalToolError::NotFound {
                tool: LocalTool::Gh
            })
        ));
    }
}
