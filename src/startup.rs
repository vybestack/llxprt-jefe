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
use crate::persistence::settings_document::{PublishedSettings, SettingsDocument};
use crate::persistence::writer::ExpectedHash;
use crate::persistence::{FilePersistenceManager, PersistencePaths};

/// Fully resolved startup paths, published settings, and their persistence manager.
#[derive(Debug)]
pub struct StartupPersistence {
    pub paths: ResolvedPaths,
    pub settings: PublishedSettings,
    pub keymap_snapshot: ActionRegistrySnapshot,
    pub settings_document: SettingsDocument,
    pub settings_expected_hash: ExpectedHash,
    keymap_diagnostic: Option<KeymapDiagnostic>,
    pub manager: FilePersistenceManager,
    /// The plugin package inventory found in the ordered roots (issue #389).
    ///
    /// Scanned exactly once here, at the boundary that already owns path
    /// resolution. Nothing downstream rescans, so what the Settings section
    /// shows and what the session composed are the same moment.
    pub plugin_inventory: Vec<crate::state::plugins_editor::PluginSnapshotRow>,
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
    // Fix the multiplexer's installation identity to the *effective* state
    // path, so a `--config <dir>` launch gets its own server instead of
    // reaching into the ambient one (issue #547).
    identity_outcome(
        &paths.state.path,
        crate::runtime::installation::initialize(&paths.state.path).map(|_| ()),
    )?;
    report_namespace_drift(&paths.state.path);
    apply_state_import(&paths.state)?;
    let (keymap, settings_document, settings_expected_hash) =
        validate_settings(&paths.settings.path)?;
    validate_state(&paths.state.path)?;
    let plugin_inventory = scan_plugin_inventory(&paths);
    let manager = FilePersistenceManager::with_paths(PersistencePaths {
        settings_path: paths.settings.path.clone(),
        state_path: paths.state.path.clone(),
    });
    Ok(StartupPersistence {
        paths,
        settings: keymap.settings,
        keymap_snapshot: keymap.composed.snapshot().clone(),
        settings_document,
        settings_expected_hash,
        keymap_diagnostic: keymap.diagnostic,
        manager,
        plugin_inventory,
    })
}

/// Scan the ordered package roots into the pure snapshot the UI projects.
///
/// A scan never fails the session: a root that cannot be read simply
/// contributes nothing, exactly as a missing root does, because an unreadable
/// package directory is not a reason to refuse to start.
fn scan_plugin_inventory(
    paths: &ResolvedPaths,
) -> Vec<crate::state::plugins_editor::PluginSnapshotRow> {
    use crate::persistence::plugin_inventory::{scan, snapshot};
    use crate::persistence::plugin_roots::{PluginRootRequest, candidate_roots};

    let roots = candidate_roots(&PluginRootRequest {
        executable_dir: std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf)),
        platform: crate::persistence::paths::Platform::current(),
        config_plugins_dir: paths.plugins.clone(),
    });
    snapshot(&scan(&roots), &crate::domain::plugin::HostTriple::current())
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

fn validate_settings(
    path: &Path,
) -> Result<(LoadedKeymap, SettingsDocument, ExpectedHash), PathError> {
    let bytes = read_optional(path)?;
    let catalog = builtin_owner_catalog()
        .map_err(|error| path_error(path, CfgCode::E005, 2, &format!("owner catalog: {error}")))?;
    let keymap = load_bytes(bytes.as_deref(), &catalog, &path.to_string_lossy())
        .map_err(|diagnostics| diagnostic_error(path, diagnostics, 2))?;
    let (source, expected) = if let Some(bytes) = bytes {
        let document = SettingsDocument::parse(&bytes)
            .map_err(|diagnostic| diagnostic_error(path, vec![*diagnostic], 2))?;
        let expected = ExpectedHash::Present(document.sha256());
        (document, expected)
    } else {
        let document = SettingsDocument::parse(b"settings_schema = 2\n")
            .map_err(|diagnostic| diagnostic_error(path, vec![*diagnostic], 2))?;
        (document, ExpectedHash::Absent)
    };
    Ok((keymap, source, expected))
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

/// Decide whether a failure to fix the installation identity stops startup.
///
/// The two failure modes deserve opposite answers. A rejected `JEFE_NAMESPACE`
/// is an operator asking for isolation we cannot give them; continuing would
/// attach them to the exact namespace they asked to be separated from, so
/// startup stops. A conflicting second initialization is refused but survivable:
/// a server may already be running under the identity resolved first, and
/// keeping it is what prevents orphaning those sessions (issue #547).
fn identity_outcome(
    path: &Path,
    result: Result<(), crate::runtime::installation::InstallationError>,
) -> Result<(), PathError> {
    use crate::runtime::installation::InstallationError;

    match result {
        Ok(()) => Ok(()),
        Err(error @ InstallationError::AlreadyResolved { .. }) => {
            tracing::warn!(%error, "keeping the installation identity resolved earlier");
            Ok(())
        }
        Err(error @ InstallationError::Override(_)) => {
            let mut diagnostic = Diagnostic::new(
                CfgCode::E001,
                Severity::Error,
                DiagnosticPath::new(path.to_string_lossy()),
                None,
                error.correction(),
            );
            error
                .to_string()
                .clone_into(&mut diagnostic.redacted_detail);
            Err(PathError {
                diagnostic: Box::new(diagnostic),
                exit_code: 2,
            })
        }
    }
}

/// Record the namespace this installation is running under, and report it if
/// it moved.
///
/// A namespace change cannot be undone once the old name is forgotten: the
/// name is the only handle on the sessions running under it. So this is
/// deliberately not fatal -- the new namespace is perfectly usable, and
/// refusing to start would strand the operator entirely instead of merely
/// telling them where their previous agents went (issue #547).
fn report_namespace_drift(state_path: &Path) {
    let identity = crate::runtime::installation::current();
    let drift =
        crate::runtime::namespace_record::reconcile(state_path, identity.origin(), identity.id());
    if let Some(report) = crate::runtime::namespace_record::describe(&drift, identity.id()) {
        tracing::warn!(%report, "multiplexer namespace changed for this installation");
    }
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

    /// A rejected `JEFE_NAMESPACE` stops startup instead of quietly falling
    /// back to the namespace the operator asked to be separated from.
    #[test]
    fn a_rejected_namespace_override_stops_startup() {
        use crate::runtime::installation::InstallationError;
        use crate::runtime::namespace::NamespaceError;

        let error = identity_outcome(
            Path::new(r"C:\work\one\state.json"),
            Err(InstallationError::Override(NamespaceError::Empty)),
        )
        .error_or_panic("a rejected override should stop startup");

        assert_eq!(error.exit_code, 2);
        assert!(
            error.diagnostic.correction.contains("JEFE_NAMESPACE"),
            "the operator must be told which variable to fix, got: {}",
            error.diagnostic.correction
        );
    }

    /// Re-initializing with a different identity is survivable: the first
    /// identity keeps its already-running server rather than being orphaned.
    #[test]
    fn a_conflicting_second_initialization_does_not_stop_startup() {
        use crate::runtime::installation::InstallationError;

        let outcome = identity_outcome(
            Path::new(r"C:\work\one\state.json"),
            Err(InstallationError::AlreadyResolved {
                active: "jefe-1111111111111111".to_owned(),
                requested: "jefe-2222222222222222".to_owned(),
            }),
        );

        assert!(outcome.is_ok(), "a conflict must not stop startup");
    }

    /// Startup must leave a record of the namespace it ran under, because a
    /// later build that computes a different one can only report the change if
    /// this build wrote down what it used.
    #[test]
    fn startup_records_the_namespace_it_ran_under() {
        let dir = unique_dir("namespace_record");
        let persistence =
            build_persistence(Some(&dir)).value_or_panic("startup should build persistence");

        let record = persistence
            .paths
            .state
            .path
            .parent()
            .unwrap_or_else(|| panic!("the state path should have a parent"))
            .join("runtime-namespace.json");
        let contents = std::fs::read_to_string(&record)
            .value_or_panic("startup should have recorded the active namespace");

        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap_or_else(|error| {
            panic!("the record must be valid JSON: {error}; got {contents}")
        });

        assert_eq!(
            parsed.get("namespace").and_then(serde_json::Value::as_str),
            Some(crate::runtime::installation::current().id().as_str()),
            "the namespace field must name the namespace actually in force, got: {contents}"
        );
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
