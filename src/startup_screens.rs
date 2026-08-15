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

#[cfg(test)]
#[path = "startup_screens_tests.rs"]
mod startup_screens_tests;
use crate::persistence::paths::ResolvedPaths;
use crate::persistence::plugin_inventory::InstalledPackage;
use crate::persistence::screen_files::{DefinitionsUnreadable, discover};
use crate::persistence::settings_document::PublishedSettings;
use crate::workbench::compose::{
    CompositionRefused, ScreenComposition, compose_screens_with_packages,
};
use crate::workbench::screens::{RegistryError, builtin_screens};

/// Why startup could not publish a screen registry.
#[derive(Debug)]
pub enum ScreenStartupError {
    /// The compiled screen table is malformed.
    Compiled(RegistryError),
    /// The definitions directory exists but could not be enumerated.
    Definitions(DefinitionsUnreadable),
    /// An enabled definition was unusable, so the candidate was refused.
    Refused(Box<CompositionRefused>),
}

impl ScreenStartupError {
    /// The process exit code this failure should produce.
    ///
    /// A malformed compiled table is this program's mistake, so it exits
    /// `EX_SOFTWARE`-adjacent `78`. Anything traceable to a file on disk exits
    /// `2`, matching every other configuration diagnostic.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Compiled(_) => 78,
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
        }
    }
}

impl std::error::Error for ScreenStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Compiled(error) => Some(error),
            Self::Definitions(error) => Some(error),
            Self::Refused(refusal) => Some(refusal.as_ref()),
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
    packages: &[InstalledPackage],
    settings: &PublishedSettings,
) -> Result<ScreenComposition, ScreenStartupError> {
    let compiled = builtin_screens().map_err(ScreenStartupError::Compiled)?;
    let candidates = discover(&paths.definitions).map_err(ScreenStartupError::Definitions)?;
    compose_screens_with_packages(&compiled, &candidates, packages, settings)
        .map_err(|refusal| ScreenStartupError::Refused(Box::new(refusal)))
}
