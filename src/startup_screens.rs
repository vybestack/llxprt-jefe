//! Screen discovery, composition, and publication at startup (issue #385).
//!
//! This is the boundary that turns resolved paths and published settings into
//! the one screen registry the rest of the program reads. It runs after
//! persistence has validated — composition needs to know which screen owners
//! settings enable — and before anything renders, because publication is atomic
//! and a renderer must never see a registry change underneath it.
//!
//! Failure here stops startup rather than degrading it. A user who enabled a
//! screen definition asked for that screen; quietly starting without it would
//! be a different program from the one they configured, and the recovery — edit
//! or disable the named file and restart — needs the diagnostic to say which
//! file and which rule.

use crate::persistence::diagnostic::Diagnostic;

#[cfg(test)]
#[path = "startup_screens_tests.rs"]
mod startup_screens_tests;
use crate::persistence::paths::ResolvedPaths;
use crate::persistence::screen_files::{DefinitionsUnreadable, discover};
use crate::persistence::settings_document::PublishedSettings;
use crate::workbench::compose::{CompositionRefused, ScreenComposition, compose_screens};
use crate::workbench::screens::{RegistryError, builtin_screens};
use crate::workbench::{RegistryAlreadyPublished, publish_screen_registry};

/// Why startup could not publish a screen registry.
#[derive(Debug)]
pub enum ScreenStartupError {
    /// The compiled screen table is malformed.
    Compiled(RegistryError),
    /// The definitions directory exists but could not be enumerated.
    Definitions(DefinitionsUnreadable),
    /// An enabled definition was unusable, so the candidate was refused.
    Refused(Box<CompositionRefused>),
    /// A registry was already published, which is an ordering mistake here.
    AlreadyPublished(RegistryAlreadyPublished),
}

impl ScreenStartupError {
    /// The process exit code this failure should produce.
    ///
    /// A malformed compiled table or a double publication is this program's
    /// mistake, so it exits `EX_SOFTWARE`-adjacent `78` like the other
    /// compiled-configuration failures. Anything traceable to a file on disk
    /// exits `2`, matching every other configuration diagnostic.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Compiled(_) | Self::AlreadyPublished(_) => 78,
            Self::Definitions(_) | Self::Refused(_) => 2,
        }
    }
}

impl std::fmt::Display for ScreenStartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compiled(error) => write!(formatter, "{error}"),
            Self::Definitions(error) => write!(formatter, "{error}"),
            Self::Refused(refusal) => write!(formatter, "{refusal}"),
            Self::AlreadyPublished(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ScreenStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Compiled(error) => Some(error),
            Self::Definitions(error) => Some(error),
            Self::Refused(refusal) => Some(refusal.as_ref()),
            Self::AlreadyPublished(error) => Some(error),
        }
    }
}

/// Discover, lower, and compose the candidate screen registry.
///
/// This is everything up to but not including publication, so it can be run
/// more than once — publication cannot.
///
/// # Errors
///
/// Returns [`ScreenStartupError`] when the compiled table is malformed, the
/// definitions directory cannot be read, or an enabled definition is unusable.
pub fn compose(
    paths: &ResolvedPaths,
    settings: &PublishedSettings,
) -> Result<ScreenComposition, ScreenStartupError> {
    let compiled = builtin_screens().map_err(ScreenStartupError::Compiled)?;
    let candidates = discover(&paths.definitions).map_err(ScreenStartupError::Definitions)?;
    compose_screens(&compiled, &candidates, settings)
        .map_err(|refusal| ScreenStartupError::Refused(Box::new(refusal)))
}

/// Compose the registry and publish it as the program's authority.
///
/// Returns the warnings composition produced — one per preserved, omitted
/// definition — so the caller can surface them without them being fatal.
///
/// # Errors
///
/// Returns [`ScreenStartupError`] for any composition failure, or when a
/// registry was already published. Nothing is published in any of those cases.
pub fn compose_and_publish(
    paths: &ResolvedPaths,
    settings: &PublishedSettings,
) -> Result<Vec<Diagnostic>, ScreenStartupError> {
    let composition = compose(paths, settings)?;
    publish_screen_registry(composition.registry).map_err(ScreenStartupError::AlreadyPublished)?;
    Ok(composition.warnings)
}
