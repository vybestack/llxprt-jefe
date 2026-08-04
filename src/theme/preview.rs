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
//!
//! The token is a pure value. Showing a theme is the manager's job
//! ([`super::ThemeManager::select`]), and holding the token is the settings
//! draft's; keeping the three verbs here means the rule about which theme to
//! return to has exactly one statement.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::ThemeId;

/// Process-unique identity of one theme preview.
///
/// Identity only. Previews are compared for sameness, never ordered: the
/// counter they are drawn from says when one was issued, which is not a fact
/// about previews worth exposing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// One in-flight theme preview and the exact theme it replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePreviewToken {
    id: PreviewId,
    generation: u64,
    prior_theme: ThemeId,
    preview_theme: ThemeId,
}

impl ThemePreviewToken {
    /// Show `preview_theme`, returning the token that undoes it.
    ///
    /// `current` is the preview being replaced, if there is one; its prior
    /// theme is carried forward, so a chain of previews still returns to the
    /// theme the user actually started from.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::StaleGeneration`] when `current` belongs to a
    /// draft that has since been replaced.
    pub fn apply(
        generation: u64,
        current: Option<&Self>,
        active: &ThemeId,
        preview_theme: ThemeId,
    ) -> Result<Self, PreviewError> {
        if let Some(token) = current {
            token.require_live(generation)?;
        }
        Ok(Self {
            id: PreviewId::next(),
            generation,
            prior_theme: current.map_or_else(|| active.clone(), |token| token.prior_theme.clone()),
            preview_theme,
        })
    }

    /// The theme a successful save makes active, consuming the token.
    #[must_use]
    pub fn adopt(self) -> ThemeId {
        self.preview_theme
    }

    /// The exact theme a cancel, discard, reload, or failed save restores.
    #[must_use]
    pub fn revert(self) -> ThemeId {
        self.prior_theme
    }

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

    /// Whether this token still belongs to the live draft.
    #[must_use]
    pub const fn is_live(&self, generation: u64) -> bool {
        self.generation == generation
    }

    const fn require_live(&self, generation: u64) -> Result<(), PreviewError> {
        if self.is_live(generation) {
            Ok(())
        } else {
            Err(PreviewError::StaleGeneration {
                token: self.generation,
                live: generation,
            })
        }
    }
}
