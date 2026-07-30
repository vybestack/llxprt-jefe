//! Startup-boundary path resolution and persistence validation.
//!
//! Normal startup consumes the same [`ResolvedPaths`] authority as recovery,
//! applies the shared source decision table (importing exactly one valid
//! distinct source into an absent target, atomically, while retaining the
//! source), and validates bounded settings/state bytes before composition.
//! Ambiguous or malformed sources block startup with the same typed
//! diagnostics and exits as recovery.

use std::path::Path;

use crate::config_owners::builtin_owner_catalog;
use crate::domain::action_registry::ActionRegistrySnapshot;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::keymap_edit::{KeymapDiagnostic, LoadedKeymap, load_bytes};
use crate::persistence::migration::migrate_state;
use crate::persistence::paths::{
    ImportDecision, InspectedSource, PathError, PhysicalIdentity, ResolvedFile, ResolvedPaths,
    SourceValidity, decide_import, import_state_source, physical_identity, resolve,
};
use crate::persistence::settings_document::PublishedSettings;
use crate::persistence::{FilePersistenceManager, PersistencePaths};

/// Fully resolved startup paths, published settings, and their persistence manager.
#[derive(Debug)]
pub struct StartupPersistence {
    pub paths: ResolvedPaths,
    pub settings: PublishedSettings,
    pub keymap_snapshot: ActionRegistrySnapshot,
    keymap_diagnostic: Option<KeymapDiagnostic>,
    pub manager: FilePersistenceManager,
}

impl StartupPersistence {
    /// Stable diagnostic code when startup replaced a malformed keymap with defaults.
    #[must_use]
    pub fn keymap_diagnostic_code(&self) -> Option<&'static str> {
        self.keymap_diagnostic
            .as_ref()
            .map(|_| KeymapDiagnostic::code())
    }

    /// Render the typed keymap diagnostic retained during compiled-default fallback.
    #[must_use]
    pub fn keymap_diagnostic_message(&self) -> Option<String> {
        self.keymap_diagnostic.as_ref().map(ToString::to_string)
    }
}

/// Resolve and validate persistence before runtime or provider composition.
pub fn build_persistence(config_dir: Option<&Path>) -> Result<StartupPersistence, PathError> {
    let paths = resolve(config_dir)?;
    apply_state_import(&paths.state)?;
    let keymap = validate_settings(&paths.settings.path)?;
    validate_state(&paths.state.path)?;
    let manager = FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: paths.settings.path.clone(),
        state_path: paths.state.path.clone(),
    });
    Ok(StartupPersistence {
        paths,
        settings: keymap.settings,
        keymap_snapshot: keymap.composed.snapshot().clone(),
        keymap_diagnostic: keymap.diagnostic,
        manager,
    })
}

fn apply_state_import(file: &ResolvedFile) -> Result<(), PathError> {
    let target = existing_identity(&file.path)?;
    let sources = inspect_sources(file)?;
    match decide_import(target.is_some(), target.as_ref(), &sources)? {
        ImportDecision::Empty => Ok(()),
        ImportDecision::Import { source } => import_state_source(&source, &file.path)
            .map(|_| ())
            .map_err(|error| import_error(&file.path, &error)),
    }
}

fn existing_identity(path: &Path) -> Result<Option<PhysicalIdentity>, PathError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => physical_identity(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_error(path, CfgCode::E001, 2, &error.to_string())),
    }
}

fn inspect_sources(file: &ResolvedFile) -> Result<Vec<InspectedSource>, PathError> {
    let mut inspected = Vec::new();
    for source in &file.sources {
        match std::fs::read(&source.path) {
            Ok(bytes) => {
                let validity = match migrate_state(&bytes) {
                    Ok(_) => SourceValidity::Valid,
                    Err(diagnostics) => SourceValidity::Malformed(
                        diagnostics
                            .first()
                            .map_or(CfgCode::E103, |diagnostic| diagnostic.code),
                    ),
                };
                inspected.push(InspectedSource::new(
                    source.path.clone(),
                    physical_identity(&source.path)?,
                    validity,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(path_error(
                    &source.path,
                    CfgCode::E001,
                    2,
                    &error.to_string(),
                ));
            }
        }
    }
    Ok(inspected)
}

fn validate_settings(path: &Path) -> Result<LoadedKeymap, PathError> {
    let bytes = read_optional(path)?;
    let catalog = builtin_owner_catalog()
        .map_err(|error| path_error(path, CfgCode::E005, 2, &format!("owner catalog: {error}")))?;
    load_bytes(bytes.as_deref(), &catalog, &path.to_string_lossy())
        .map_err(|diagnostics| diagnostic_error(path, diagnostics, 2))
}

fn validate_state(path: &Path) -> Result<(), PathError> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(());
    };
    migrate_state(&bytes)
        .map(|_| ())
        .map_err(|diagnostics| diagnostic_error(path, diagnostics, 2))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, PathError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_error(path, CfgCode::E001, 2, &error.to_string())),
    }
}

fn import_error(path: &Path, error: &crate::persistence::paths::StateImportError) -> PathError {
    error.diagnostic().map_or_else(
        || {
            path_error(
                path,
                CfgCode::E104,
                error.exit_code(),
                "state import failed",
            )
        },
        |diagnostic| PathError {
            diagnostic: Box::new(diagnostic.clone()),
            exit_code: error.exit_code(),
        },
    )
}

fn diagnostic_error(path: &Path, diagnostics: Vec<Diagnostic>, exit_code: u8) -> PathError {
    diagnostics.into_iter().next().map_or_else(
        || path_error(path, CfgCode::E103, exit_code, "document validation failed"),
        |diagnostic| PathError {
            diagnostic: Box::new(diagnostic),
            exit_code,
        },
    )
}

fn path_error(path: &Path, code: CfgCode, exit_code: u8, detail: &str) -> PathError {
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(path.to_string_lossy()),
        None,
        "run jefe config validate and jefe config migrate-state",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    PathError {
        diagnostic: Box::new(diagnostic),
        exit_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::paths::{PathCandidate, PathProvenance};

    trait TestResultExt<T, E> {
        fn value_or_panic(self, context: &str) -> T;
        fn error_or_panic(self, context: &str) -> E;
    }

    impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }

        fn error_or_panic(self, context: &str) -> E {
            match self {
                Ok(_) => panic!("{context}: expected error"),
                Err(error) => error,
            }
        }
    }

    fn unique_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jefe_startup_{label}_{}_{}",
            std::process::id(),
            counter()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).value_or_panic("create test dir");
        dir
    }

    fn counter() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    const SCHEMA1_STATE: &str = "{\n  \"schema_version\": 1,\n  \"repositories\": [],\n  \"agents\": [],\n  \"selected_repository_index\": null,\n  \"selected_agent_index\": null\n}\n";

    #[test]
    fn build_persistence_resolves_explicit_dir_without_creating_files() {
        let dir = unique_dir("valid");
        let startup = build_persistence(Some(&dir)).value_or_panic("valid dir should build");
        assert_eq!(startup.paths.settings.path, dir.join("settings.toml"));
        assert_eq!(startup.paths.state.path, dir.join("state.json"));
        assert_eq!(startup.paths.themes, dir.join("themes"));
        assert!(!startup.paths.settings.path.exists());
        assert!(!startup.paths.state.path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_persistence_blocks_malformed_state_without_writing() {
        let dir = unique_dir("malformed");
        std::fs::write(dir.join("state.json"), "{ malformed state bytes\n")
            .value_or_panic("seed malformed state");
        let error = build_persistence(Some(&dir)).error_or_panic("malformed state must block");
        assert_eq!(error.exit_code, 2);
        assert_eq!(error.diagnostic.code, CfgCode::E103);
        let bytes = std::fs::read(dir.join("state.json")).value_or_panic("state must remain");
        assert_eq!(bytes, b"{ malformed state bytes\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_imports_single_valid_source_into_absent_target() {
        let dir = unique_dir("import");
        let source_path = dir.join("legacy-state.json");
        std::fs::write(&source_path, SCHEMA1_STATE).value_or_panic("seed source");
        let file = ResolvedFile {
            path: dir.join("state.json"),
            provenance: PathProvenance::PlatformDefault,
            sources: vec![PathCandidate {
                path: source_path.clone(),
                provenance: PathProvenance::HistoricalLinuxDataState,
            }],
        };
        apply_state_import(&file).value_or_panic("single valid source must import");
        let imported = std::fs::read_to_string(dir.join("state.json"))
            .value_or_panic("target must be written");
        assert!(imported.contains("\"state_schema\": 2"));
        let retained = std::fs::read_to_string(&source_path).value_or_panic("source must remain");
        assert_eq!(retained, SCHEMA1_STATE, "source must stay byte-identical");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_blocks_when_target_and_distinct_source_both_exist() {
        let dir = unique_dir("ambiguous");
        let source_path = dir.join("legacy-state.json");
        let target_path = dir.join("state.json");
        std::fs::write(&source_path, SCHEMA1_STATE).value_or_panic("seed source");
        std::fs::write(&target_path, SCHEMA1_STATE).value_or_panic("seed target");
        let file = ResolvedFile {
            path: target_path.clone(),
            provenance: PathProvenance::PlatformDefault,
            sources: vec![PathCandidate {
                path: source_path,
                provenance: PathProvenance::HistoricalLinuxDataState,
            }],
        };
        let error = apply_state_import(&file).error_or_panic("distinct source must block");
        assert_eq!(error.exit_code, 3);
        assert_eq!(error.diagnostic.code, CfgCode::E001);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_validates_schema1_local_tilde_state_without_rewriting_bytes() {
        let home = if cfg!(windows) {
            std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
        } else {
            std::env::var_os("HOME")
        };
        assert!(home.is_some(), "test host must provide a home directory");
        let dir = unique_dir("tilde");
        let state_path = dir.join("state.json");
        let bytes = r#"{
  "schema_version": 1,
  "repositories": [{
    "id": "home-repo",
    "name": "Home",
    "slug": "home",
    "base_dir": "~/projects/jefe",
    "default_profile": "",
    "agent_ids": []
  }],
  "agents": [],
  "selected_repository_index": 0,
  "selected_agent_index": null
}
"#;
        std::fs::write(&state_path, bytes.as_bytes()).value_or_panic("seed tilde state");
        validate_state(&state_path).value_or_panic("local ~ state must validate on startup");
        let retained =
            std::fs::read(&state_path).value_or_panic("schema-1 bytes must remain readable");
        assert_eq!(
            retained,
            bytes.as_bytes(),
            "normal startup must not rewrite schema-1 bytes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_initial_keymap_retains_bytes_and_uses_compiled_defaults() {
        use crate::domain::{
            action_registry::Resolution, input_context::ContextStack, keymap::Chord,
        };
        let dir = unique_dir("malformed-keymap");
        let source = br#"settings_schema = 2
[appearance]
theme = "green-screen"
override_agent_theme = true
[workbench]
initial_screen = "core.dashboard"
enabled_screens = ["core.dashboard"]
screen_order = ["core.dashboard"]
[agents."core.llxprt"]
enabled = false
[keymap.dashboard]
"dashboard.navigate-down" = 7
"#;
        std::fs::write(dir.join("settings.toml"), source).value_or_panic("seed keymap");
        let startup = build_persistence(Some(&dir)).value_or_panic("keymap fallback must start");
        assert_eq!(startup.keymap_diagnostic_code(), Some("KEY-E401"));
        assert!(
            startup
                .keymap_diagnostic_message()
                .is_some_and(|message| message.starts_with("KEY-E401:"))
        );
        assert_eq!(
            startup.settings.appearance.theme.as_deref(),
            Some("green-screen")
        );
        assert_eq!(startup.settings.appearance.override_agent_theme, Some(true));
        let initial_screen = startup.settings.workbench.initial_screen.as_ref();
        assert_eq!(
            initial_screen.map(crate::domain::Id::as_str),
            Some("core.dashboard")
        );
        let owner = crate::domain::Id::parse("core.llxprt").value_or_panic("agent owner id");
        assert_eq!(
            startup
                .settings
                .agents
                .get(&owner)
                .and_then(|entry| entry.enabled),
            Some(false)
        );
        assert!(startup.settings.keymap.is_empty());
        let chord = Chord::parse("j").value_or_panic("default chord");
        let stack = ContextStack::from_ordered(["dashboard", "global"], false)
            .value_or_panic("dashboard stack");
        assert!(matches!(
            startup.keymap_snapshot.resolve(&chord, &stack),
            Resolution::Dispatch { .. }
        ));
        assert_eq!(
            std::fs::read(dir.join("settings.toml")).value_or_panic("retained settings"),
            source
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_settings_syntax_still_blocks_startup() {
        let dir = unique_dir("malformed-settings-syntax");
        let source = b"settings_schema = 2\n[keymap.dashboard\n";
        std::fs::write(dir.join("settings.toml"), source).value_or_panic("seed settings");

        let error = build_persistence(Some(&dir)).error_or_panic("syntax must block startup");

        assert_eq!(error.exit_code, 2);
        assert_eq!(
            std::fs::read(dir.join("settings.toml")).value_or_panic("retained settings"),
            source
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
