//! Startup-boundary helpers for wiring persistence and managers.
//!
//! These functions keep the TUI-launching `main` thin by moving testable
//! construction/validation logic into the library. In particular,
//! [`build_persistence`] performs fail-fast validation of an explicit
//! `--config` directory so that an unwritable path produces a clear, actionable
//! error instead of silent apparent data loss mid-session.
//!
//! @requirement REQ-TECH-005

use crate::persistence::{
    FilePersistenceManager, PersistenceError, PersistencePaths, resolve_paths,
    resolve_paths_from_dir, validate_config_dir,
};

/// Build the persistence manager for the resolved config directory.
///
/// When `config_dir` is `Some(dir)` (i.e. `--config <dir>` was supplied) the
/// directory is validated fail-fast via [`validate_config_dir`]: it is created
/// if missing, confirmed to be a directory, and probed for writability of both
/// `settings.toml` and `state.json`. When `config_dir` is `None` the default
/// platform/env paths are used with no extra validation, matching the existing
/// behavior for the implicit config location.
///
/// # Errors
///
/// Returns [`PersistenceError::InvalidConfigDir`] when an explicit config
/// directory cannot be created or written to.
pub fn build_persistence(
    config_dir: Option<&std::path::Path>,
) -> Result<FilePersistenceManager, PersistenceError> {
    let paths = resolve_persistence_paths(config_dir)?;
    Ok(FilePersistenceManager::with_paths(paths))
}

/// Resolve [`PersistencePaths`] for the given config directory, validating an
/// explicit directory before returning.
fn resolve_persistence_paths(
    config_dir: Option<&std::path::Path>,
) -> Result<PersistencePaths, PersistenceError> {
    match config_dir {
        Some(dir) => {
            validate_config_dir(dir)?;
            Ok(resolve_paths_from_dir(dir))
        }
        None => Ok(resolve_paths()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistenceError;

    trait TestResultExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }
    }

    fn expect_error<E: std::fmt::Debug>(result: Result<(), E>, context: &str) -> E {
        match result {
            Ok(()) => panic!("{context}: expected error"),
            Err(error) => error,
        }
    }

    fn unique_dir(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("jefe_test_startup_{label}_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        root.join(label)
    }

    #[test]
    fn build_persistence_validates_explicit_dir_and_rejects_regular_file() {
        // A path that exists as a regular file must be rejected fail-fast with
        // a context-rich error mentioning the config path.
        let temp = unique_dir("regular_file");
        let root = temp.parent().map(std::path::Path::to_path_buf);
        let _ = std::fs::remove_file(&temp);
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::write(&temp, "not a directory").value_or_panic("should seed regular file");

        let error = expect_error(
            build_persistence(Some(&temp)).map(|_| ()),
            "should reject regular file at startup",
        );
        let PersistenceError::InvalidConfigDir { path, reason } = &error else {
            panic!("expected InvalidConfigDir, got {error:?}");
        };
        assert_eq!(path, &temp, "error must mention the config path");
        assert!(
            reason.contains("not a directory"),
            "reason should explain it is not a directory, got: {reason}"
        );

        let _ = std::fs::remove_file(&temp);
        if let Some(root) = root {
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn build_persistence_succeeds_for_valid_explicit_dir() {
        // A freshly-created writable explicit directory should build a manager
        // whose paths root under that directory.
        let temp = unique_dir("valid_dir");
        let root = temp.parent().map(std::path::Path::to_path_buf);
        let _ = std::fs::remove_dir_all(&temp);

        let manager =
            build_persistence(Some(&temp)).value_or_panic("valid dir should build manager");

        assert_eq!(
            manager.paths_ref().settings_path,
            temp.join("settings.toml")
        );
        assert_eq!(manager.paths_ref().state_path, temp.join("state.json"));

        let _ = std::fs::remove_dir_all(&temp);
        if let Some(root) = root {
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn build_persistence_none_uses_default_paths() {
        // No explicit dir must not error and must not validate anything.
        let manager = build_persistence(None).value_or_panic("None should build default manager");
        let expected = crate::persistence::resolve_paths();
        assert_eq!(manager.paths_ref().settings_path, expected.settings_path);
        assert_eq!(manager.paths_ref().state_path, expected.state_path);
    }
}
