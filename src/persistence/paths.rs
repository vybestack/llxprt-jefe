//! Single authority for persistence paths, physical identity, and import decisions.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, PATH_LIMIT, Severity};
use super::migration::migrate_state;
use super::writer::{
    AtomicWrite, BackupPolicy, DraftBytes, ExpectedHash, Freshness, WriteError, WriteOutcome,
};

/// Platform whose standard configuration locations should be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Linux,
    Windows,
    Unsupported,
}

impl Platform {
    /// Return the compile-target platform.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unsupported
        }
    }
}

/// Explicit path environment captured before resolution.
#[derive(Debug, Clone, Default)]
pub struct PathEnvironment {
    pub jefe_settings_path: Option<OsString>,
    pub jefe_state_path: Option<OsString>,
    pub jefe_config_dir: Option<OsString>,
    pub jefe_state_dir: Option<OsString>,
    pub xdg_config_home: Option<OsString>,
    pub xdg_state_home: Option<OsString>,
    pub xdg_data_home: Option<OsString>,
    pub home: Option<OsString>,
    pub appdata: Option<OsString>,
    pub local_appdata: Option<OsString>,
}

impl PathEnvironment {
    /// Capture only variables owned by the path authority.
    #[must_use]
    pub fn capture() -> Self {
        Self {
            jefe_settings_path: std::env::var_os("JEFE_SETTINGS_PATH"),
            jefe_state_path: std::env::var_os("JEFE_STATE_PATH"),
            jefe_config_dir: std::env::var_os("JEFE_CONFIG_DIR"),
            jefe_state_dir: std::env::var_os("JEFE_STATE_DIR"),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME"),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
            home: std::env::var_os("HOME"),
            appdata: std::env::var_os("APPDATA"),
            local_appdata: std::env::var_os("LOCALAPPDATA"),
        }
    }
}

/// Inputs kept explicit so tests never mutate process environment.
#[derive(Debug, Clone)]
pub struct PathResolutionRequest {
    pub config_dir: Option<PathBuf>,
    pub platform: Platform,
    pub current_dir: PathBuf,
}

/// Reason a selected path was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathProvenance {
    ConfigArgument,
    SettingsPathEnvironment,
    StatePathEnvironment,
    ConfigDirectoryEnvironment,
    StateDirectoryEnvironment,
    PlatformDefault,
    HistoricalLinuxDataState,
}

/// One potential one-way schema-1 import source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCandidate {
    pub path: PathBuf,
    pub provenance: PathProvenance,
}

/// Selected target and its bounded migration sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFile {
    pub path: PathBuf,
    pub provenance: PathProvenance,
    pub sources: Vec<PathCandidate>,
}

/// Complete path selection shared by startup and recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub settings: ResolvedFile,
    pub state: ResolvedFile,
    pub definitions: PathBuf,
    pub plugins: PathBuf,
    pub themes: PathBuf,
}

/// Typed path or source-decision failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    pub diagnostic: Box<Diagnostic>,
    pub exit_code: u8,
}

/// Resolve selected files and source candidates without touching the filesystem.
pub fn resolve_from(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
) -> Result<ResolvedPaths, PathError> {
    if let Some(config_dir) = &request.config_dir {
        return resolve_isolated(config_dir, &request.current_dir);
    }
    validate_environment(environment)?;
    let defaults = platform_defaults(request, environment)?;
    let settings = select_settings(request, environment, &defaults)?;
    let mut state = select_state(request, environment, &defaults)?;
    if let Some(source) = defaults.historical_state
        && source != state.path
    {
        state.sources.push(PathCandidate {
            path: source,
            provenance: PathProvenance::HistoricalLinuxDataState,
        });
    }
    let root = settings.path.parent().ok_or_else(|| {
        path_error(
            CfgCode::E001,
            2,
            &settings.path,
            "settings path has no parent",
        )
    })?;
    Ok(ResolvedPaths {
        definitions: root.join("definitions"),
        plugins: root.join("plugins"),
        themes: root.join("themes"),
        settings,
        state,
    })
}

fn resolve_isolated(config_dir: &Path, current_dir: &Path) -> Result<ResolvedPaths, PathError> {
    let root = normalize_path(config_dir, current_dir)?;
    Ok(ResolvedPaths {
        settings: resolved(root.join("settings.toml"), PathProvenance::ConfigArgument),
        state: resolved(root.join("state.json"), PathProvenance::ConfigArgument),
        definitions: root.join("definitions"),
        plugins: root.join("plugins"),
        themes: root.join("themes"),
    })
}

fn resolved(path: PathBuf, provenance: PathProvenance) -> ResolvedFile {
    ResolvedFile {
        path,
        provenance,
        sources: Vec::new(),
    }
}

struct PlatformDefaults {
    settings: PathBuf,
    state: PathBuf,
    historical_state: Option<PathBuf>,
}

fn platform_defaults(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
) -> Result<PlatformDefaults, PathError> {
    match request.platform {
        Platform::Macos => macos_defaults(request, environment),
        Platform::Linux => linux_defaults(request, environment),
        Platform::Windows => windows_defaults(request, environment),
        Platform::Unsupported => Err(path_error(
            CfgCode::E001,
            2,
            &request.current_dir,
            "platform has no standard Jefe persistence directory",
        )),
    }
}

fn macos_defaults(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
) -> Result<PlatformDefaults, PathError> {
    let home = required_env_path("HOME", environment.home.as_ref(), request)?;
    let root = home.join("Library/Application Support/jefe");
    Ok(PlatformDefaults {
        settings: root.join("settings.toml"),
        state: root.join("state.json"),
        historical_state: None,
    })
}

fn linux_defaults(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
) -> Result<PlatformDefaults, PathError> {
    let home = required_env_path("HOME", environment.home.as_ref(), request)?;
    let config = optional_env_path(
        "XDG_CONFIG_HOME",
        environment.xdg_config_home.as_ref(),
        request,
    )?
    .unwrap_or_else(|| home.join(".config"));
    let state = optional_env_path(
        "XDG_STATE_HOME",
        environment.xdg_state_home.as_ref(),
        request,
    )?
    .unwrap_or_else(|| home.join(".local/state"));
    let data = optional_env_path("XDG_DATA_HOME", environment.xdg_data_home.as_ref(), request)?
        .unwrap_or_else(|| home.join(".local/share"));
    Ok(PlatformDefaults {
        settings: config.join("jefe/settings.toml"),
        state: state.join("jefe/state.json"),
        historical_state: Some(data.join("jefe/state.json")),
    })
}

fn windows_defaults(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
) -> Result<PlatformDefaults, PathError> {
    let config = required_env_path("APPDATA", environment.appdata.as_ref(), request)?;
    let state = required_env_path("LOCALAPPDATA", environment.local_appdata.as_ref(), request)?;
    Ok(PlatformDefaults {
        settings: config.join("jefe/settings.toml"),
        state: state.join("jefe/state.json"),
        historical_state: None,
    })
}

fn select_settings(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
    defaults: &PlatformDefaults,
) -> Result<ResolvedFile, PathError> {
    if let Some(path) = optional_env_path(
        "JEFE_SETTINGS_PATH",
        environment.jefe_settings_path.as_ref(),
        request,
    )? {
        return Ok(resolved(path, PathProvenance::SettingsPathEnvironment));
    }
    if let Some(path) = optional_env_path(
        "JEFE_CONFIG_DIR",
        environment.jefe_config_dir.as_ref(),
        request,
    )? {
        return Ok(resolved(
            path.join("settings.toml"),
            PathProvenance::ConfigDirectoryEnvironment,
        ));
    }
    Ok(resolved(
        defaults.settings.clone(),
        PathProvenance::PlatformDefault,
    ))
}

fn select_state(
    request: &PathResolutionRequest,
    environment: &PathEnvironment,
    defaults: &PlatformDefaults,
) -> Result<ResolvedFile, PathError> {
    if let Some(path) = optional_env_path(
        "JEFE_STATE_PATH",
        environment.jefe_state_path.as_ref(),
        request,
    )? {
        return Ok(resolved(path, PathProvenance::StatePathEnvironment));
    }
    if let Some(path) = optional_env_path(
        "JEFE_STATE_DIR",
        environment.jefe_state_dir.as_ref(),
        request,
    )? {
        return Ok(resolved(
            path.join("state.json"),
            PathProvenance::StateDirectoryEnvironment,
        ));
    }
    Ok(resolved(
        defaults.state.clone(),
        PathProvenance::PlatformDefault,
    ))
}

fn validate_environment(environment: &PathEnvironment) -> Result<(), PathError> {
    for (name, value) in [
        ("JEFE_SETTINGS_PATH", &environment.jefe_settings_path),
        ("JEFE_STATE_PATH", &environment.jefe_state_path),
        ("JEFE_CONFIG_DIR", &environment.jefe_config_dir),
        ("JEFE_STATE_DIR", &environment.jefe_state_dir),
        ("XDG_CONFIG_HOME", &environment.xdg_config_home),
        ("XDG_STATE_HOME", &environment.xdg_state_home),
        ("XDG_DATA_HOME", &environment.xdg_data_home),
        ("HOME", &environment.home),
        ("APPDATA", &environment.appdata),
        ("LOCALAPPDATA", &environment.local_appdata),
    ] {
        if let Some(value) = value {
            validate_os_value(name, value)?;
        }
    }
    Ok(())
}

fn required_env_path(
    name: &str,
    value: Option<&OsString>,
    request: &PathResolutionRequest,
) -> Result<PathBuf, PathError> {
    optional_env_path(name, value, request)?.ok_or_else(|| {
        path_error(
            CfgCode::E001,
            2,
            &request.current_dir,
            &format!("{name} is required"),
        )
    })
}

fn optional_env_path(
    name: &str,
    value: Option<&OsString>,
    request: &PathResolutionRequest,
) -> Result<Option<PathBuf>, PathError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let text = validate_os_value(name, value)?;
    normalize_path(Path::new(text), &request.current_dir).map(Some)
}

fn validate_os_value<'a>(name: &str, value: &'a OsStr) -> Result<&'a str, PathError> {
    let Some(text) = value.to_str() else {
        return Err(path_error(
            CfgCode::E001,
            2,
            Path::new("/"),
            &format!("{name} is not Unicode"),
        ));
    };
    if text.is_empty() || text.len() > PATH_LIMIT {
        return Err(path_error(
            CfgCode::E001,
            2,
            Path::new("/"),
            &format!("{name} is empty or over limit"),
        ));
    }
    Ok(text)
}

fn normalize_path(path: &Path, current_dir: &Path) -> Result<PathBuf, PathError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };
    let text = path
        .to_str()
        .ok_or_else(|| path_error(CfgCode::E001, 2, &path, "path is not Unicode"))?;
    if text.is_empty() || text.len() > PATH_LIMIT {
        return Err(path_error(
            CfgCode::E001,
            2,
            &path,
            "path is empty or over limit",
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(path_error(CfgCode::E001, 2, &path, "path escapes its root"));
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

/// Platform-native identity for an existing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PhysicalFileKey {
    Unix { device: u64, inode: u64 },
    Windows { volume_serial: u64, file_index: u64 },
}

/// Canonical lexical path plus physical key when the leaf exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalIdentity {
    canonical_path: PathBuf,
    file_key: Option<PhysicalFileKey>,
}

impl PhysicalIdentity {
    /// Borrow the canonical existing or nearest-ancestor-derived path.
    #[must_use]
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    /// Compare aliases by file key where available, otherwise canonical path.
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> bool {
        match (self.file_key, other.file_key) {
            (Some(left), Some(right)) => left == right,
            _ => self.canonical_path == other.canonical_path,
        }
    }
}

/// Resolve physical identity without creating a missing directory or leaf.
pub fn physical_identity(path: &Path) -> Result<PhysicalIdentity, PathError> {
    match std::fs::canonicalize(path) {
        Ok(canonical_path) => existing_identity(canonical_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing_identity(path),
        Err(error) => Err(path_error(
            CfgCode::E001,
            2,
            path,
            &format!("cannot canonicalize path: {error}"),
        )),
    }
}

fn existing_identity(canonical_path: PathBuf) -> Result<PhysicalIdentity, PathError> {
    let metadata = std::fs::metadata(&canonical_path).map_err(|error| {
        path_error(
            CfgCode::E001,
            2,
            &canonical_path,
            &format!("cannot read metadata: {error}"),
        )
    })?;
    #[cfg(unix)]
    let file_key = Some(unix_file_key(&metadata));
    #[cfg(windows)]
    let file_key = windows_file_key(&metadata);
    #[cfg(not(any(unix, windows)))]
    let file_key = None;
    Ok(PhysicalIdentity {
        canonical_path,
        file_key,
    })
}

fn missing_identity(path: &Path) -> Result<PhysicalIdentity, PathError> {
    let mut ancestor = path.to_path_buf();
    let mut suffix = Vec::new();
    loop {
        match std::fs::canonicalize(&ancestor) {
            Ok(mut canonical) => {
                for component in suffix.iter().rev() {
                    canonical.push(component);
                }
                return Ok(PhysicalIdentity {
                    canonical_path: canonical,
                    file_key: None,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .map(OsStr::to_os_string)
                    .ok_or_else(|| {
                        path_error(CfgCode::E001, 2, path, "path has no existing ancestor")
                    })?;
                suffix.push(name);
                if !ancestor.pop() {
                    return Err(path_error(
                        CfgCode::E001,
                        2,
                        path,
                        "path has no existing ancestor",
                    ));
                }
            }
            Err(error) => {
                return Err(path_error(
                    CfgCode::E001,
                    2,
                    path,
                    &format!("cannot inspect ancestor: {error}"),
                ));
            }
        }
    }
}

#[cfg(unix)]
fn unix_file_key(metadata: &std::fs::Metadata) -> PhysicalFileKey {
    use std::os::unix::fs::MetadataExt;
    PhysicalFileKey::Unix {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(windows)]
fn windows_file_key(metadata: &std::fs::Metadata) -> Option<PhysicalFileKey> {
    use std::os::windows::fs::MetadataExt;
    Some(PhysicalFileKey::Windows {
        volume_serial: u64::from(metadata.volume_serial_number()?),
        file_index: metadata.file_index()?,
    })
}

/// Result of validating one discovered source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceValidity {
    Valid,
    Malformed(CfgCode),
}

/// Source paired with identity and static validation outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedSource {
    path: PathBuf,
    identity: PhysicalIdentity,
    validity: SourceValidity,
}

impl InspectedSource {
    /// Construct one inspected source record.
    #[must_use]
    pub fn new(path: PathBuf, identity: PhysicalIdentity, validity: SourceValidity) -> Self {
        Self {
            path,
            identity,
            validity,
        }
    }
}

/// Read/import decision made before any write occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportDecision {
    Empty,
    Import { source: PathBuf },
}

/// Decide source import after physical deduplication and validation.
pub fn decide_import(
    target_exists: bool,
    target_identity: Option<&PhysicalIdentity>,
    sources: &[InspectedSource],
) -> Result<ImportDecision, PathError> {
    let distinct = distinct_sources(target_identity, sources);
    if target_exists && !distinct.is_empty() {
        return Err(path_error(
            CfgCode::E001,
            3,
            &distinct[0].path,
            "target and source are physically distinct",
        ));
    }
    if let Some(source) = distinct
        .iter()
        .find(|source| matches!(source.validity, SourceValidity::Malformed(_)))
    {
        let SourceValidity::Malformed(code) = source.validity else {
            return Err(path_error(
                CfgCode::E103,
                2,
                &source.path,
                "malformed source",
            ));
        };
        return Err(path_error(code, 2, &source.path, "malformed source"));
    }
    match distinct.as_slice() {
        [] => Ok(ImportDecision::Empty),
        [source] => Ok(ImportDecision::Import {
            source: source.path.clone(),
        }),
        [first, ..] => Err(path_error(
            CfgCode::E001,
            3,
            &first.path,
            "multiple distinct sources",
        )),
    }
}

/// Failure while reading, migrating, serializing, or writing an import.
#[derive(Debug)]
pub enum StateImportError {
    /// Static source or serialization diagnostics with their recovery exit.
    Diagnostics {
        diagnostics: Vec<Diagnostic>,
        exit_code: u8,
    },
    /// Atomic writer failure retaining the immutable schema-2 draft.
    Write(WriteError),
}

impl StateImportError {
    /// Return the recovery command exit code.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Diagnostics { exit_code, .. } => *exit_code,
            Self::Write(_) => 4,
        }
    }

    /// Borrow the primary typed diagnostic.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        match self {
            Self::Diagnostics { diagnostics, .. } => diagnostics.first(),
            Self::Write(error) => Some(error.diagnostic()),
        }
    }
}

/// Import one physically distinct state source into an absent selected target.
///
/// The source is read and migrated entirely in memory, then the schema-2
/// candidate is installed through the sole atomic writer authority. The source
/// remains byte-for-byte unchanged and no schema-1 backup is created because it
/// is not the selected authority being replaced.
pub fn import_state_source(source: &Path, target: &Path) -> Result<WriteOutcome, StateImportError> {
    let source_bytes = std::fs::read(source)
        .map_err(|error| import_diagnostic(source, CfgCode::E104, 4, error.to_string()))?;
    let migrated =
        migrate_state(&source_bytes).map_err(|diagnostics| StateImportError::Diagnostics {
            diagnostics,
            exit_code: 2,
        })?;
    let draft = migrated
        .to_canonical_json()
        .map_err(|error| import_diagnostic(source, CfgCode::E104, 4, error.to_string()))?;
    let operation = AtomicWrite {
        target: target.to_path_buf(),
        draft: DraftBytes::new(draft),
        expected: ExpectedHash::Absent,
        revision: migrated.state().revision,
        backup: BackupPolicy::None,
    };
    super::writer::write(operation, |_| Freshness::Current).map_err(StateImportError::Write)
}

fn import_diagnostic(
    path: &Path,
    code: CfgCode,
    exit_code: u8,
    detail: String,
) -> StateImportError {
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(path.to_string_lossy()),
        None,
        "retain the source and retry the explicit state migration",
    );
    diagnostic.redacted_detail = detail;
    StateImportError::Diagnostics {
        diagnostics: vec![diagnostic],
        exit_code,
    }
}

fn distinct_sources<'a>(
    target: Option<&PhysicalIdentity>,
    sources: &'a [InspectedSource],
) -> Vec<&'a InspectedSource> {
    let mut distinct = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for source in sources {
        if target.is_some_and(|target| target.equivalent(&source.identity))
            || distinct
                .iter()
                .any(|seen: &&InspectedSource| seen.identity.equivalent(&source.identity))
        {
            continue;
        }
        if seen_paths.insert(source.identity.canonical_path.clone()) {
            distinct.push(source);
        }
    }
    distinct
}

fn path_error(code: CfgCode, exit_code: u8, path: &Path, detail: &str) -> PathError {
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(path.to_string_lossy()),
        None,
        "select one valid operating-system-standard path",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    PathError {
        diagnostic: Box::new(diagnostic),
        exit_code,
    }
}
