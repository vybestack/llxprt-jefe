//! The boundary that decides which installation this process *is* (#547).
//!
//! [`super::namespace`] is deliberately pure: it turns a path into an identity
//! and does nothing else. Something still has to read the environment, consult
//! the persistence layer and pick the one path that counts. That happens here,
//! exactly once per process, so the answer cannot drift between the code that
//! starts a multiplexer server and the code that later looks for it.
//!
//! Resolution order:
//! 1. `JEFE_NAMESPACE` — an explicit, deliberate override (A/B testing two
//!    builds, or quarantining a session pool). Honored verbatim and reported
//!    as such, because an override that silently degraded to the derived
//!    namespace would reattach the operator to the sessions they asked to be
//!    kept away from.
//! 2. The resolved `state.json` location, which is what `--config` moves.
//!
//! The result is stored in a write-once cell. [`initialize`] is the explicit
//! startup call that records it from the *effective* paths (including
//! `--config`); [`current`] falls back to the environment-derived paths so that
//! tests and library consumers that never call `initialize` still get a
//! deterministic answer rather than a panic.

use super::namespace::{InstallationIdentity, NamespaceError};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Environment variable holding a deliberate namespace override.
///
/// Every message that names this variable interpolates it from here, so
/// renaming it cannot leave an operator following stale instructions.
pub const NAMESPACE_OVERRIDE_ENV: &str = "JEFE_NAMESPACE";

static ACTIVE: OnceLock<InstallationIdentity> = OnceLock::new();

/// Why an installation identity could not be established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallationError {
    /// `JEFE_NAMESPACE` was set to something unusable as a server name.
    Override(NamespaceError),
    /// [`initialize`] was called twice with different answers.
    ///
    /// The first answer is kept, because by the time this happens a
    /// multiplexer server may already exist under it and silently switching
    /// would orphan every session on it.
    AlreadyResolved { active: String, requested: String },
}

impl std::fmt::Display for InstallationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Override(error) => {
                write!(formatter, "{NAMESPACE_OVERRIDE_ENV} is unusable: {error}")
            }
            Self::AlreadyResolved { active, requested } => write!(
                formatter,
                "installation identity is already resolved as `{active}`; \
                 refusing to switch to `{requested}` mid-process"
            ),
        }
    }
}

impl InstallationError {
    /// What the operator should do about this failure.
    ///
    /// The guidance deliberately does not restate the validation rules. Those
    /// live with the validation in [`NamespaceError`] and reach the operator
    /// through this error's own `Display`; repeating them here would give them
    /// a second place to drift out of sync with the code that enforces them.
    #[must_use]
    pub fn correction(&self) -> String {
        match self {
            Self::Override(_) => format!(
                "unset {NAMESPACE_OVERRIDE_ENV}, or set it to a value that satisfies the rule \
                 reported above"
            ),
            Self::AlreadyResolved { active, .. } => {
                format!("keep using `{active}`; restart jefe to adopt a different namespace")
            }
        }
    }
}

impl std::error::Error for InstallationError {}

/// Record the installation this process was launched as.
///
/// `state_path` is the *effective* `state.json` location, so a `--config <dir>`
/// launch keys its multiplexer on that directory rather than on the ambient
/// environment.
///
/// Calling this twice with the same answer is harmless. Calling it twice with
/// different answers is an error and the first answer wins.
///
/// # Errors
///
/// Returns [`InstallationError::Override`] when `JEFE_NAMESPACE` is set to a
/// value that cannot name a server, and [`InstallationError::AlreadyResolved`]
/// when the identity was already fixed to something else.
pub fn initialize(state_path: &Path) -> Result<&'static InstallationIdentity, InstallationError> {
    let requested = resolve(state_path)?;

    if let Err(rejected) = ACTIVE.set(requested) {
        let Some(active) = ACTIVE.get() else {
            // Unreachable: `set` only fails once the cell is populated.
            return Err(InstallationError::AlreadyResolved {
                active: String::new(),
                requested: rejected.id().as_str().to_owned(),
            });
        };
        return reconcile(active, &rejected).map(|()| active);
    }

    let active = current();
    if active.origin().is_override() {
        // Deliberate isolation: say so at warn level, because the operator has
        // opted out of the installation's real session pool and anything they
        // started without the override will look like it vanished.
        tracing::warn!(
            installation = %active,
            "{NAMESPACE_OVERRIDE_ENV} is in effect; this jefe is isolated from the \
             session pool of the config it was launched from"
        );
    } else {
        tracing::info!(installation = %active, "resolved multiplexer installation identity");
    }
    Ok(active)
}

/// The installation this process belongs to.
///
/// If [`initialize`] has not run, this resolves from the environment-derived
/// persistence paths. That keeps unit tests and embedded uses deterministic
/// without making every caller thread a path through.
#[must_use]
pub fn current() -> &'static InstallationIdentity {
    ACTIVE.get_or_init(|| {
        let state_path = default_state_path();
        resolve(&state_path).unwrap_or_else(|error| {
            // Reached only when nobody called `initialize` (which reports the
            // same problem as a startup error). Falling back is still better
            // than panicking in a library path, but it must be loud.
            tracing::error!(
                %error,
                "falling back to the state-path-derived namespace; \
                 sessions started under the intended override will not be visible"
            );
            InstallationIdentity::for_state_path(&state_path)
        })
    })
}

/// A fresh, single-use identity that cannot collide with a real installation.
///
/// Used by test seams that need their own multiplexer server. It extends the
/// active installation's identity so stray servers are still traceable back to
/// the tree that spawned them.
#[must_use]
pub fn isolated_run() -> InstallationIdentity {
    InstallationIdentity::isolated_run(&active_state_path())
}

/// The state path behind the active identity.
///
/// An override carries no path of its own, so fall back to the resolved
/// persistence paths for that case.
pub fn active_state_path() -> PathBuf {
    current()
        .origin()
        .state_path()
        .map_or_else(default_state_path, Path::to_path_buf)
}

fn default_state_path() -> PathBuf {
    crate::persistence::resolve_paths().state_path
}

fn resolve(state_path: &Path) -> Result<InstallationIdentity, InstallationError> {
    resolve_with(
        std::env::var(NAMESPACE_OVERRIDE_ENV).ok().as_deref(),
        state_path,
    )
}

/// The pure core of [`resolve`], split out so the precedence is testable
/// without mutating process environment variables (`set_var` is `unsafe` under
/// edition 2024 and forbidden here).
pub(super) fn resolve_with(
    namespace_override: Option<&str>,
    state_path: &Path,
) -> Result<InstallationIdentity, InstallationError> {
    match namespace_override.filter(|raw| !raw.trim().is_empty()) {
        Some(raw) => InstallationIdentity::from_override(raw).map_err(InstallationError::Override),
        None => Ok(InstallationIdentity::for_state_path(state_path)),
    }
}

/// Whether a second [`initialize`] agrees with the identity already in force.
///
/// Split out from the `OnceLock` plumbing so the refusal can be tested without
/// a process-global side effect.
pub(super) fn reconcile(
    active: &InstallationIdentity,
    requested: &InstallationIdentity,
) -> Result<(), InstallationError> {
    if active.id() == requested.id() {
        return Ok(());
    }
    Err(InstallationError::AlreadyResolved {
        active: active.id().as_str().to_owned(),
        requested: requested.id().as_str().to_owned(),
    })
}
