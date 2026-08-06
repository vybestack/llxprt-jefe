//! Provider process-environment construction and secret redaction
//! (issue #390 CW-10, CW10-14).
//!
//! A provider process begins from an **empty** environment and receives only
//! what the closed contract permits: the provider directory plus a fixed
//! platform system-bins `PATH`, contained `HOME`/`TMPDIR`, a locale, the
//! manifest-declared nonsecret names, and any explicitly declared secret
//! environment bindings. Secret values are resolved **only** from declared
//! host-environment references and travel only into the owning `Configure`
//! payload (or an explicitly declared secret binding). No resolved secret value
//! is ever placed in application state, a log, retained stderr, an observation
//! report, or a diagnostic: [`ResolvedSecrets`] collects them solely so a
//! [`Redactor`] can scrub every provider-owned observation surface.
//!
//! This module is pure and injectable. [`HostEnv`] is the seam tests use to
//! supply deterministic host values; [`ProcessHostEnv`] is the production
//! resolver that reads `std::env::var`. No process handle lives here.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use super::identifiers::EnvName;

/// The literal scrubbed into every redacted observation surface.
pub const REDACTION_PLACEHOLDER: &str = "[REDACTED]";

/// Fixed platform system-bin directories prepended after the provider directory
/// to form the provider `PATH`.
///
/// On Unix these are the conventional system binary locations. On Windows the
/// conventional `%SystemRoot%` system directories are used because the provider
/// environment does not inherit the host environment.
#[must_use]
pub fn system_bin_paths() -> Vec<PathBuf> {
    cfg_system_bins()
}

#[cfg(unix)]
fn cfg_system_bins() -> Vec<PathBuf> {
    [PathBuf::from("/usr/bin"), PathBuf::from("/bin")].to_vec()
}

#[cfg(windows)]
fn cfg_system_bins() -> Vec<PathBuf> {
    [
        PathBuf::from(r"C:\Windows\System32"),
        PathBuf::from(r"C:\Windows\System32\Wbem"),
        PathBuf::from(r"C:\Windows"),
    ]
    .to_vec()
}

#[cfg(not(any(unix, windows)))]
fn cfg_system_bins() -> Vec<PathBuf> {
    Vec::new()
}

/// Injectable host-environment resolver.
///
/// The production resolver reads `std::env::var`; tests supply deterministic
/// values without touching the real environment.
pub trait HostEnv {
    /// Resolve one host environment variable, if it is present.
    fn get(&self, name: &str) -> Option<String>;
}

/// Production host-environment resolver backed by `std::env::var`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessHostEnv;

impl HostEnv for ProcessHostEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// The provider environment specification (CW10-14).
///
/// All maps are keyed by validated environment-variable names. Secret sources
/// name a host environment variable to resolve and never carry a value.
#[derive(Debug, Clone, Default)]
pub struct ProviderEnvironment {
    /// Directory the selected provider binary lives in; first `PATH` entry.
    pub provider_dir: PathBuf,
    /// Declared nonsecret environment bindings with their literal values.
    pub nonsecret: BTreeMap<EnvName, String>,
    /// Explicit secret environment bindings: the key is the variable to set,
    /// the value is the host variable to resolve.
    pub secret_env: BTreeMap<EnvName, EnvName>,
    /// `Configure` secret sources: the key is the owning binding name (the
    /// `Configure.secrets` key), the value is the host variable to resolve.
    pub configure_secret_sources: BTreeMap<EnvName, EnvName>,
}

/// Why environment construction failed.
///
/// No variant carries a secret value. A missing declared source names the
/// binding and the unresolved host variable, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentError {
    /// A declared secret source could not be resolved from the host.
    UnresolvedSecret {
        /// The owning binding the value was destined for.
        binding: String,
        /// The host variable that was declared but absent.
        source: String,
    },
    /// The caller supplied a `Configure` secret the manifest did not declare as
    /// a host-environment source. The supervisor is the sole secret resolver:
    /// every `Configure` secret must come from a declared
    /// `configure_secret_sources` reference. No secret value is carried.
    UndeclaredConfigureSecret {
        /// The caller-supplied binding the manifest did not declare.
        binding: String,
    },
}

impl fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedSecret { binding, source } => write!(
                formatter,
                "declared secret binding {binding:?} references unset host variable {source:?}"
            ),
            Self::UndeclaredConfigureSecret { binding } => write!(
                formatter,
                "configure secret {binding:?} was supplied by the caller but is not a declared host secret source"
            ),
        }
    }
}

impl std::error::Error for EnvironmentError {}

/// The constructed provider process environment plus the resolved secret values
/// collected for redaction.
///
/// [`Debug`](std::fmt::Debug) is intentionally **not** derived: the resolved
/// secret values must never reach a log or diagnostic. Access to the values is
/// only through [`Self::redactor`].
pub struct ProcessEnv {
    vars: BTreeMap<OsString, OsString>,
    secrets: ResolvedSecrets,
}

impl ProcessEnv {
    /// The environment variables to apply to the provider command, in stable
    /// order.
    pub fn vars(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.vars
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str()))
    }

    /// The number of environment variables.
    #[must_use]
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// Whether the environment is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Borrow the value of one variable, if present.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&OsStr> {
        self.vars.get(OsStr::new(key)).map(OsString::as_os_str)
    }

    /// A redactor scrubbed against every resolved secret value.
    #[must_use]
    pub fn redactor(&self) -> Redactor {
        self.secrets.redactor()
    }

    /// Whether any secret was resolved.
    #[must_use]
    pub fn has_secrets(&self) -> bool {
        !self.secrets.values.is_empty()
    }
}

/// Resolved secret material, collected solely for redaction.
///
/// Never print this type: its values are secret. [`Debug`](std::fmt::Debug) is
/// a manual, redacted implementation.
pub struct ResolvedSecrets {
    values: Vec<String>,
}

impl ResolvedSecrets {
    /// Build a redactor over every resolved value.
    #[must_use]
    pub fn redactor(&self) -> Redactor {
        Redactor::new(self.values.clone())
    }

    /// How many distinct secret values were resolved.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether no secret was resolved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl fmt::Debug for ResolvedSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ResolvedSecrets {{ {} value(s) [redacted] }}",
            self.values.len()
        )
    }
}

/// Scrub known secret values out of retained provider-owned text.
///
/// Used for retained stderr and observation reports so a secret that a
/// misbehaving provider echoes never reaches an operator surface. Longer
/// values are replaced first so a secret that is a prefix of another is not
/// half-scrubbed.
pub struct Redactor {
    values: Vec<String>,
}

impl Redactor {
    /// Construct a redactor over a set of secret values.
    ///
    /// Empty values are ignored because they match nothing.
    #[must_use]
    pub fn new(values: Vec<String>) -> Self {
        let mut filtered: Vec<String> = values.into_iter().filter(|v| !v.is_empty()).collect();
        filtered.sort_by_key(|value| std::cmp::Reverse(value.len()));
        Self { values: filtered }
    }

    /// Whether the redactor has any value to scrub.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Redact every known secret value in `text`, replacing it with
    /// [`REDACTION_PLACEHOLDER`].
    #[must_use]
    pub fn redact<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.values.is_empty() {
            return Cow::Borrowed(text);
        }
        let mut result = text.to_owned();
        let mut changed = false;
        for value in &self.values {
            if result.contains(value.as_str()) {
                result = result.replace(value.as_str(), REDACTION_PLACEHOLDER);
                changed = true;
            }
        }
        if changed {
            Cow::Owned(result)
        } else {
            Cow::Borrowed(text)
        }
    }
}

/// Build the provider process environment from the specification.
///
/// The environment starts empty and gains only: the provider-directory-led
/// `PATH`, contained `HOME` and `TMPDIR`, the locale, the declared nonsecret
/// bindings, and the explicitly declared secret bindings. Declared
/// `Configure` secret sources are resolved and collected for the returned
/// [`ProcessEnv::redactor`] but are **not** placed in the process environment:
/// they travel only in the owning `Configure` payload.
///
/// # Errors
///
/// Returns [`EnvironmentError::UnresolvedSecret`] when a declared secret source
/// is absent from the host environment.
pub fn build_process_env<E: HostEnv>(
    spec: &ProviderEnvironment,
    home: &Path,
    tmpdir: &Path,
    locale: &str,
    host_env: &E,
) -> Result<ProcessEnv, EnvironmentError> {
    let mut vars: BTreeMap<OsString, OsString> = BTreeMap::new();
    let mut secrets = Vec::new();

    vars.insert(
        OsString::from("PATH"),
        build_path(&spec.provider_dir).into_os_string(),
    );
    insert_path(&mut vars, "HOME", home);
    insert_path(&mut vars, "TMPDIR", tmpdir);
    insert_str(&mut vars, "LC_ALL", locale);
    insert_str(&mut vars, "LANG", locale);

    for (name, value) in &spec.nonsecret {
        insert_str(&mut vars, name.as_str(), value);
    }

    for (binding, source) in &spec.secret_env {
        let value = resolve_secret(host_env, binding, source)?;
        secrets.push(value.clone());
        insert_str(&mut vars, binding.as_str(), &value);
    }

    for (binding, source) in &spec.configure_secret_sources {
        let value = resolve_secret(host_env, binding, source)?;
        secrets.push(value);
    }

    secrets.sort();
    secrets.dedup();
    Ok(ProcessEnv {
        vars,
        secrets: ResolvedSecrets { values: secrets },
    })
}

/// Resolve the `Configure` secret map (keyed by binding) from the specification.
///
/// Each value is the resolved host-environment reference; the same values are
/// already present in the [`ProcessEnv`] redactor. Returns an empty map when no
/// secret source is declared.
///
/// # Errors
///
/// Returns [`EnvironmentError::UnresolvedSecret`] when a declared source is
/// absent from the host environment.
pub fn resolve_configure_secrets<E: HostEnv>(
    spec: &ProviderEnvironment,
    host_env: &E,
) -> Result<BTreeMap<EnvName, String>, EnvironmentError> {
    let mut secrets = BTreeMap::new();
    for (binding, source) in &spec.configure_secret_sources {
        let value = resolve_secret(host_env, binding, source)?;
        secrets.insert(binding.clone(), value);
    }
    Ok(secrets)
}

fn resolve_secret<E: HostEnv>(
    host_env: &E,
    binding: &EnvName,
    source: &EnvName,
) -> Result<String, EnvironmentError> {
    host_env
        .get(source.as_str())
        .ok_or_else(|| EnvironmentError::UnresolvedSecret {
            binding: binding.to_string(),
            source: source.to_string(),
        })
}

fn build_path(provider_dir: &Path) -> PathBuf {
    let mut joined = provider_dir.to_string_lossy().into_owned();
    for bin in system_bin_paths() {
        joined.push(path_separator());
        joined.push_str(&bin.to_string_lossy());
    }
    PathBuf::from(joined)
}

#[cfg(unix)]
fn path_separator() -> char {
    ':'
}

#[cfg(not(unix))]
fn path_separator() -> char {
    ';'
}

fn insert_str(vars: &mut BTreeMap<OsString, OsString>, key: &str, value: &str) {
    vars.insert(OsString::from(key), OsString::from(value));
}

fn insert_path(vars: &mut BTreeMap<OsString, OsString>, key: &str, value: &Path) {
    vars.insert(OsString::from(key), value.as_os_str().to_owned());
}
