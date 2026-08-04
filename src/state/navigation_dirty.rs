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

use crate::domain::Id;
use crate::domain::effects::{Correlation, SemanticKey};
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
    /// The owner will save this draft, and only a completion carrying this
    /// exact identity resolves the guard.
    Owner {
        /// Who is saving. A completion from anything else is not this save.
        owner: Id,
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
    /// The owner has been asked to save and has not yet said what it registered.
    ///
    /// The reducer cannot allocate a correlation identifier — the pending
    /// ledger does that, after this transition commits — so there is a moment
    /// where the guard knows what it asked for but not yet which attempt is
    /// running. Nothing can resolve the guard during it.
    SaveRequested {
        /// Who was asked.
        owner: Id,
        /// The operation that was asked for.
        semantic_key: SemanticKey,
        /// The draft the request was for.
        draft: DraftToken,
    },
    /// A specific save attempt is running; only its exact identity resolves it.
    ///
    /// Owner, semantic key, and both generations are not enough on their own:
    /// a retry of the same operation on the same screen matches all of them, so
    /// a late answer from the abandoned attempt would resolve the live one. The
    /// correlation identifier is what distinguishes two attempts, which is why
    /// the guard waits to be told it rather than guessing.
    Saving {
        /// The exact identity the resolving completion must carry.
        expected: Box<Correlation>,
        /// The draft this attempt is saving.
        draft: DraftToken,
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
    pub(super) fn save_requested(
        &mut self,
        owner: Id,
        semantic_key: SemanticKey,
        draft: DraftToken,
    ) {
        self.phase = GuardPhase::SaveRequested {
            owner,
            semantic_key,
            draft,
        };
    }

    /// Record which attempt the owner actually registered.
    ///
    /// Refuses anything that does not answer the request that was made, so a
    /// stray registration cannot take over the guard.
    pub(super) fn save_started(&mut self, correlation: &Correlation) -> bool {
        let GuardPhase::SaveRequested {
            owner,
            semantic_key,
            draft,
        } = &self.phase
        else {
            return false;
        };
        if &correlation.owner != owner || &correlation.semantic_key != semantic_key {
            return false;
        }
        self.phase = GuardPhase::Saving {
            expected: Box::new(correlation.clone()),
            draft: *draft,
        };
        true
    }

    /// Move to the recovery state, keeping the draft and the pending navigation.
    pub(super) fn failed(&mut self, detail: String) {
        self.phase = GuardPhase::Failed { detail };
    }

    /// Whether `correlation` is the answer this guard is waiting for.
    ///
    /// `held` is the draft the instance still holds; a save whose draft has
    /// since been replaced is answering about work that no longer exists.
    #[must_use]
    pub(super) fn awaits(&self, correlation: &Correlation, held: Option<DraftToken>) -> bool {
        let GuardPhase::Saving { expected, draft } = &self.phase else {
            return false;
        };
        expected.matches(correlation) && held == Some(*draft)
    }

    /// Whether a save attempt is in flight, by request or by registration.
    #[must_use]
    pub(super) const fn is_saving(&self) -> bool {
        matches!(
            self.phase,
            GuardPhase::SaveRequested { .. } | GuardPhase::Saving { .. }
        )
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
    /// Run the declared save; its completion must carry this exact identity.
    Save {
        /// Who is saving.
        owner: Id,
        /// The operation to run.
        semantic_key: SemanticKey,
        /// The draft being saved.
        draft: DraftToken,
    },
    /// Abandon this draft and restore the base it was taken from.
    RestoreBase {
        /// The draft to abandon.
        draft: DraftToken,
    },
}
