//! Reversible, generation-bound theme preview (issue #387, CW07-03).
//!
//! Previewing a theme is the one settings edit the user has to *see* to judge,
//! so the theme has to change before it is saved. What must never happen is
//! that cancelling leaves the session wearing a theme nobody chose.
//!
//! The token is what makes that impossible. It names the theme to return to as
//! well as the theme being shown, so reverting does not depend on the manager
//! remembering anything, and replacing a preview keeps the *original* prior
//! theme rather than the theme the previous preview happened to be showing.
//! Binding the token to a generation means a token issued for a draft that has
//! since been reloaded, discarded, or replaced cannot reach back and repaint
//! the session.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::ThemeId;

/// Process-unique identity of one theme preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PreviewId(u64);

static NEXT_PREVIEW: AtomicU64 = AtomicU64::new(1);

impl PreviewId {
    /// Allocate the next distinct preview identity.
    #[must_use]
    pub fn next() -> Self {
        Self(NEXT_PREVIEW.fetch_add(1, Ordering::Relaxed))
    }

    /// The raw counter value, for goldens and diagnostics.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for PreviewId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "preview-{}", self.0)
    }
}

/// One in-flight theme preview and the exact theme it replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePreviewToken {
    id: PreviewId,
    generation: u64,
    prior_theme: ThemeId,
    preview_theme: ThemeId,
}

impl ThemePreviewToken {
    /// This preview's identity.
    #[must_use]
    pub const fn id(&self) -> PreviewId {
        self.id
    }

    /// The draft generation this preview belongs to.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The theme the very first preview of this draft replaced.
    #[must_use]
    pub const fn prior_theme(&self) -> &ThemeId {
        &self.prior_theme
    }

    /// The theme currently being shown.
    #[must_use]
    pub const fn preview_theme(&self) -> &ThemeId {
        &self.preview_theme
    }
}

/// Why a preview operation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// The token belongs to a draft that has since been replaced.
    StaleGeneration {
        /// The generation the token carries.
        token: u64,
        /// The generation that is live now.
        live: u64,
    },
    /// The named theme is not installed, so it cannot be shown.
    Unavailable(ThemeId),
}

impl fmt::Display for PreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleGeneration { token, live } => write!(
                formatter,
                "theme preview generation {token} is not the live generation {live}"
            ),
            Self::Unavailable(theme) => write!(formatter, "theme {theme} is not installed"),
        }
    }
}

impl std::error::Error for PreviewError {}

/// Check that a token still belongs to the live draft.
pub(super) const fn check_generation(
    token: &ThemePreviewToken,
    live: u64,
) -> Result<(), PreviewError> {
    if token.generation == live {
        Ok(())
    } else {
        Err(PreviewError::StaleGeneration {
            token: token.generation,
            live,
        })
    }
}

/// Build the token that undoes showing `preview_theme`.
///
/// `current` is the preview being replaced, if there is one; its prior theme is
/// carried forward so a chain of previews still returns to where it started.
pub(super) fn issue(
    generation: u64,
    current: Option<&ThemePreviewToken>,
    active: ThemeId,
    preview_theme: ThemeId,
) -> ThemePreviewToken {
    ThemePreviewToken {
        id: PreviewId::next(),
        generation,
        prior_theme: current.map_or(active, |token| token.prior_theme.clone()),
        preview_theme,
    }
}
