//! The host dirty guard that stands between a draft and a screen change
//! (issue #386).
//!
//! Leaving a screen with unsaved work is the same question everywhere, so it
//! has one answer here rather than one answer per screen. Navigation raises the
//! guard, the user chooses, and only then does the screen change — or not.
//!
//! What the guard deliberately does **not** know is what saving *means*. The
//! screen that owns the draft declares that as a [`SaveIntent`], and the
//! settings shell that follows this capability owns its own draft, writer, and
//! completion. The guard's whole job is to hold the navigation back until the
//! owner reports success, so a save that fails cannot take the user off the
//! screen holding their work.
//!
//! There is deliberately no `DiscardIntent` beside [`SaveIntent`]. Saving
//! varies by owner — there may be nowhere to save to at all — but abandoning a
//! draft is always available and always means the same thing: the owner
//! restores the base its draft was taken from. A type whose only job is to say
//! that would carry no information.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::effects::SemanticKey;
use crate::workbench::PanelId;

use super::navigation::NavIntent;

/// Opaque identity of one in-progress draft.
///
/// Identity rather than content, so the guard can name the draft it is holding
/// without holding the draft: a completion that names a draft the owner has
/// since replaced is answering about something that no longer exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DraftToken(u64);

static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);

impl DraftToken {
    /// Allocate the next distinct draft identity.
    #[must_use]
    pub fn next() -> Self {
        Self(NEXT_DRAFT.fetch_add(1, Ordering::Relaxed))
    }

    /// The raw counter value, for goldens and diagnostics.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DraftToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "draft-{}", self.0)
    }
}

/// What the screen that owns a draft says its Save does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveIntent {
    /// This draft has nowhere to save to, so the guard offers only Discard and
    /// Cancel and says why Save is unavailable.
    ///
    /// An unsent comment is the shipped example: there is no server-side draft
    /// store, so the honest choices are to abandon it or to go back to it.
    Unavailable {
        /// Why Save cannot be offered, shown beside the disabled control.
        reason: &'static str,
    },
    /// The owner will save this draft, and a completion carrying this semantic
    /// key resolves the guard.
    Owner {
        /// The operation the owner will run, and the key its completion carries.
        semantic_key: SemanticKey,
    },
}

impl SaveIntent {
    /// Whether the guard may offer Save.
    #[must_use]
    pub const fn can_save(&self) -> bool {
        matches!(self, Self::Owner { .. })
    }
}

/// Whether the current instance holds unsaved work.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DirtyState {
    /// Nothing is unsaved; navigation proceeds without a guard.
    #[default]
    Clean,
    /// The owner holds a draft, and leaving must be confirmed.
    Dirty {
        /// Which draft is held.
        draft: DraftToken,
        /// What this owner's Save does, which the guard never interprets.
        save: SaveIntent,
    },
}

impl DirtyState {
    /// The held draft, if there is one.
    #[must_use]
    pub const fn draft(&self) -> Option<DraftToken> {
        match self {
            Self::Clean => None,
            Self::Dirty { draft, .. } => Some(*draft),
        }
    }

    /// Whether leaving this instance must be confirmed.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        matches!(self, Self::Dirty { .. })
    }
}

/// What the user chose in the dirty guard.
///
/// Retry is not a fourth choice: after a failed save the guard offers the same
/// Save under a Retry label, so choosing Save again reruns the owner's save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyChoice {
    /// Run the owner's save, and navigate only if it succeeds.
    Save,
    /// Abandon the draft and navigate.
    Discard,
    /// Stay, keeping the draft and the focus the guard interrupted.
    Cancel,
}

/// How far the guard has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardPhase {
    /// Awaiting the user's choice.
    Choosing,
    /// The owner's save is running; a completion carrying this key resolves it.
    Saving {
        /// The key the resolving completion must carry.
        semantic_key: SemanticKey,
    },
    /// The save failed; the draft is intact and Retry, Discard, and Cancel remain.
    Failed {
        /// The redacted reason, shown in the recovery state.
        detail: String,
    },
}

/// A screen change waiting on the current instance's draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyGuard {
    pending: NavIntent,
    restore_focus: PanelId,
    phase: GuardPhase,
}

impl DirtyGuard {
    /// Raise a guard over `pending`, remembering the focus it interrupted.
    #[must_use]
    pub(super) const fn raised(pending: NavIntent, restore_focus: PanelId) -> Self {
        Self {
            pending,
            restore_focus,
            phase: GuardPhase::Choosing,
        }
    }

    /// The navigation this guard is holding back.
    #[must_use]
    pub const fn pending(&self) -> &NavIntent {
        &self.pending
    }

    /// The focus Cancel restores.
    #[must_use]
    pub const fn restore_focus(&self) -> PanelId {
        self.restore_focus
    }

    /// How far the guard has got.
    #[must_use]
    pub const fn phase(&self) -> &GuardPhase {
        &self.phase
    }

    /// Move to awaiting the owner's save.
    pub(super) fn saving(&mut self, semantic_key: SemanticKey) {
        self.phase = GuardPhase::Saving { semantic_key };
    }

    /// Move to the recovery state, keeping the draft and the pending navigation.
    pub(super) fn failed(&mut self, detail: String) {
        self.phase = GuardPhase::Failed { detail };
    }

    /// The key a completion must carry to resolve this guard, if one is running.
    #[must_use]
    pub(super) fn awaited_key(&self) -> Option<&SemanticKey> {
        match &self.phase {
            GuardPhase::Saving { semantic_key } => Some(semantic_key),
            GuardPhase::Choosing | GuardPhase::Failed { .. } => None,
        }
    }
}

/// What the draft's owner must do after a guard transition.
///
/// The guard never runs a save and never touches a draft. It says what is now
/// required, and the owner — which is the only thing that knows what its draft
/// is — does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftAction {
    /// Nothing is required of the owner.
    None,
    /// Run the declared save; its completion must carry this semantic key.
    Save {
        /// The operation to run.
        semantic_key: SemanticKey,
    },
    /// Abandon this draft and restore the base it was taken from.
    RestoreBase {
        /// The draft to abandon.
        draft: DraftToken,
    },
}
