//! Provider-free configuration recovery boundary.
//!
//! Recovery resolves persistence paths and renders deterministic command output
//! without initializing logging, terminal UI, providers, probes, or runtimes.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::domain::OwnerCatalog;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::migration::{migrate_settings, migrate_state};
use crate::persistence::paths::{
    PathCandidate, PathEnvironment, PathError, PathProvenance, PathResolutionRequest,
    PhysicalFileKey, PhysicalIdentity, Platform, ResolvedFile, ResolvedPaths, physical_identity,
    resolve_from,
};

/// Fully rendered recovery result consumed by the process entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
}

impl RecoveryOutput {
    fn success(stdout: String) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            exit_code: 0,
        }
    }

    fn failure(error: PathError) -> Self {
        Self::diagnostics(vec![*error.diagnostic], error.exit_code)
    }

    fn diagnostics(mut diagnostics: Vec<Diagnostic>, exit_code: u8) -> Self {
        diagnostics.sort();
        Self {
            stdout: String::new(),
            stderr: render_diagnostics(&diagnostics),
            exit_code,
        }
    }
}

/// Resolve and print selected and migration-source paths without starting services.
#[must_use]
pub fn run_path(config_dir: Option<&Path>) -> RecoveryOutput {
    let paths = match resolve_recovery_paths(config_dir) {
        Ok(paths) => paths,
        Err(output) => return output,
    };
    match PathReport::build(&paths).and_then(render_json) {
        Ok(stdout) => RecoveryOutput::success(stdout),
        Err(error) => RecoveryOutput::failure(error),
    }
}

/// Parse settings and state statically without writing or starting services.
#[must_use]
pub fn run_validate(config_dir: Option<&Path>) -> RecoveryOutput {
    let paths = match resolve_recovery_paths(config_dir) {
        Ok(paths) => paths,
        Err(output) => return output,
    };
    let settings = match validate_settings(&paths.settings.path) {
        Ok(report) => report,
        Err(output) => return output,
    };
    let state = match validate_state(&paths.state.path) {
        Ok(report) => report,
        Err(output) => return output,
    };
    match render_validation(ValidationReport {
        schema: 1,
        settings,
        state,
        diagnostics: Vec::new(),
    }) {
        Ok(stdout) => RecoveryOutput::success(stdout),
        Err(error) => RecoveryOutput::failure(error),
    }
}

fn resolve_recovery_paths(config_dir: Option<&Path>) -> Result<ResolvedPaths, RecoveryOutput> {
    let current_dir = std::env::current_dir().map_err(|error| current_directory_failure(&error))?;
    let request = PathResolutionRequest {
        config_dir: config_dir.map(Path::to_path_buf),
        platform: Platform::current(),
        current_dir,
    };
    resolve_from(&request, &PathEnvironment::capture()).map_err(RecoveryOutput::failure)
}

#[derive(Debug, Serialize)]
struct PathReport {
    settings: FileReport,
    state: FileReport,
    definitions: String,
    plugins: String,
    themes: String,
}

impl PathReport {
    fn build(paths: &ResolvedPaths) -> Result<Self, PathError> {
        Ok(Self {
            settings: FileReport::build(&paths.settings)?,
            state: FileReport::build(&paths.state)?,
            definitions: display_path(&paths.definitions),
            plugins: display_path(&paths.plugins),
            themes: display_path(&paths.themes),
        })
    }
}

#[derive(Debug, Serialize)]
struct FileReport {
    selected: String,
    canonical: String,
    physical_identity: IdentityReport,
    provenance: &'static str,
    legacy: Vec<SourceReport>,
}

impl FileReport {
    fn build(file: &ResolvedFile) -> Result<Self, PathError> {
        let identity = physical_identity(&file.path)?;
        let legacy = file
            .sources
            .iter()
            .map(SourceReport::build)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            selected: display_path(&file.path),
            canonical: display_path(identity.canonical_path()),
            physical_identity: IdentityReport::from(&identity),
            provenance: provenance_name(file.provenance),
            legacy,
        })
    }
}

#[derive(Debug, Serialize)]
struct SourceReport {
    path: String,
    canonical: String,
    physical_identity: IdentityReport,
    provenance: &'static str,
}

impl SourceReport {
    fn build(source: &PathCandidate) -> Result<Self, PathError> {
        let identity = physical_identity(&source.path)?;
        Ok(Self {
            path: display_path(&source.path),
            canonical: display_path(identity.canonical_path()),
            physical_identity: IdentityReport::from(&identity),
            provenance: provenance_name(source.provenance),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum IdentityReport {
    PathOnly,
    Unix { device: u64, inode: u64 },
    Windows { volume_serial: u64, file_index: u64 },
}

impl From<&PhysicalIdentity> for IdentityReport {
    fn from(identity: &PhysicalIdentity) -> Self {
        match identity.file_key() {
            Some(PhysicalFileKey::Unix { device, inode }) => Self::Unix { device, inode },
            Some(PhysicalFileKey::Windows {
                volume_serial,
                file_index,
            }) => Self::Windows {
                volume_serial,
                file_index,
            },
            None => Self::PathOnly,
        }
    }
}

fn provenance_name(provenance: PathProvenance) -> &'static str {
    match provenance {
        PathProvenance::ConfigArgument => "config_argument",
        PathProvenance::SettingsPathEnvironment => "settings_path_environment",
        PathProvenance::StatePathEnvironment => "state_path_environment",
        PathProvenance::ConfigDirectoryEnvironment => "config_directory_environment",
        PathProvenance::StateDirectoryEnvironment => "state_directory_environment",
        PathProvenance::PlatformDefault => "platform_default",
        PathProvenance::HistoricalLinuxDataState => "historical_linux_data_state",
    }
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    schema: u8,
    settings: DocumentReport,
    state: DocumentReport,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
struct DocumentReport {
    selected: String,
    status: &'static str,
    source_schema: Option<u8>,
    migrated_in_memory: bool,
    effective_theme: Option<String>,
    skipped_semantic_owners: Vec<String>,
    revision: Option<u64>,
}

fn validate_settings(path: &Path) -> Result<DocumentReport, RecoveryOutput> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(missing_document(path));
    };
    let migration = migrate_settings(&bytes, &OwnerCatalog::default())
        .map_err(|diagnostics| RecoveryOutput::diagnostics(diagnostics, 2))?;
    let published = migration.published();
    Ok(DocumentReport {
        selected: display_path(path),
        status: "valid",
        source_schema: Some(if migration.was_migrated() { 1 } else { 2 }),
        migrated_in_memory: migration.was_migrated(),
        effective_theme: published.appearance.theme.clone(),
        skipped_semantic_owners: published
            .dormant
            .iter()
            .map(|item| item.path.join("."))
            .collect(),
        revision: None,
    })
}

fn validate_state(path: &Path) -> Result<DocumentReport, RecoveryOutput> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(missing_document(path));
    };
    let migration =
        migrate_state(&bytes).map_err(|diagnostics| RecoveryOutput::diagnostics(diagnostics, 2))?;
    Ok(DocumentReport {
        selected: display_path(path),
        status: "valid",
        source_schema: Some(if migration.was_migrated() { 1 } else { 2 }),
        migrated_in_memory: migration.was_migrated(),
        effective_theme: None,
        skipped_semantic_owners: Vec::new(),
        revision: Some(migration.state().revision),
    })
}

fn missing_document(path: &Path) -> DocumentReport {
    DocumentReport {
        selected: display_path(path),
        status: "missing",
        source_schema: None,
        migrated_in_memory: false,
        effective_theme: None,
        skipped_semantic_owners: Vec::new(),
        revision: None,
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, RecoveryOutput> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RecoveryOutput::failure(recovery_error(
            path.to_path_buf(),
            &format!("cannot read selected document: {error}"),
        ))),
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn render_validation(report: ValidationReport) -> Result<String, PathError> {
    serde_json::to_string_pretty(&report).map_err(|error| {
        recovery_error(
            PathBuf::from("/"),
            &format!("cannot render validation output: {error}"),
        )
    })
}

fn render_json(report: PathReport) -> Result<String, PathError> {
    serde_json::to_string_pretty(&report).map_err(|error| {
        recovery_error(
            PathBuf::from("/"),
            &format!("cannot render recovery output: {error}"),
        )
    })
}

fn render_diagnostics(diagnostics: &[Diagnostic]) -> String {
    match serde_json::to_string_pretty(diagnostics) {
        Ok(rendered) => rendered,
        Err(_) => "[{\"code\":\"CFG-E104\",\"severity\":\"error\",\"path\":\"/\",\"span\":null,\"owner\":null,\"owner_version\":null,\"provenance\":[],\"correction\":\"retry recovery after correcting the output failure\",\"redacted_detail\":\"cannot render recovery diagnostics\"}]".to_owned(),
    }
}

fn current_directory_failure(error: &std::io::Error) -> RecoveryOutput {
    RecoveryOutput::failure(recovery_error(
        PathBuf::from("/"),
        &format!("cannot read current directory: {error}"),
    ))
}

fn recovery_error(path: PathBuf, detail: &str) -> PathError {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E104,
        Severity::Error,
        DiagnosticPath::new(path.to_string_lossy()),
        None,
        "retry recovery after correcting the filesystem failure",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    PathError {
        diagnostic: Box::new(diagnostic),
        exit_code: 4,
    }
}
