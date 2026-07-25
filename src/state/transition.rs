//! Bounded reducer transitions and pending effect correlation records
//! (issue #381 CW01-10/CW01-11).
//!
//! A committed transition carries the next state plus at most
//! [`MAX_TRANSITION_EFFECTS`] ordered post-commit effects. Pending effects are
//! bounded typed records keyed by semantic identity; a completion applies only
//! when all five correlation fields match its pending record exactly,
//! otherwise the state is left untouched.

use crate::domain::Id;
pub use crate::domain::effects::MAX_TRANSITION_EFFECTS;
use crate::domain::effects::{
    Correlation, CorrelationId, Effect, IssuedEffect, RetryPolicy, SemanticKey,
};

use super::AppState;

/// One committed reducer step: the next state plus ordered post-commit
/// effects, each carrying the exact correlation registered before commit.
#[derive(Debug, Clone)]
pub struct Transition {
    pub next_state: AppState,
    pub effects: Vec<IssuedEffect>,
}

/// Commit a reducer transition in place and return its staged post-commit
/// effects for execution after all state access is released.
///
/// On a rejected commit ([`TransitionError::EffectLimitExceeded`]) the
/// untouched state is reinstalled with a typed CFG-E008 error message and no
/// effects are returned — the transition never committed, so nothing may
/// execute.
pub fn commit_in_place(
    slot: &mut AppState,
    message: crate::messages::AppMessage,
) -> Vec<IssuedEffect> {
    match std::mem::take(slot).apply_message(message) {
        Ok(transition) => {
            *slot = transition.next_state;
            transition.effects
        }
        Err(TransitionError::EffectLimitExceeded { state, attempted }) => {
            *slot = *state;
            slot.error_message = Some(format!(
                "CFG-E008: transition staged {attempted} effects; the bound is {MAX_TRANSITION_EFFECTS}"
            ));
            Vec::new()
        }
    }
}

/// Commit a transition at a pure apply site that must not stage effects.
///
/// Pure UI paths (focus toggles, scrolling, direct reducer nudges) commit
/// through this helper; any staged effect reaching such a site is a contract
/// violation and is surfaced as a typed state error instead of being dropped
/// or executed out of order. Effect-staging messages must flow through the
/// composition funnels, which use [`commit_in_place`] and execute the
/// returned effects after releasing state access.
pub fn commit_pure_site(slot: &mut AppState, message: crate::messages::AppMessage) {
    let staged = commit_in_place(slot, message);
    reject_unexecuted_effects(slot, staged);
}

/// Surface staged effects that reached a site which cannot execute them.
///
/// The effects are not executed and not silently dropped: each one is
/// reported through the state error channel so the violation is observable.
pub fn reject_unexecuted_effects(slot: &mut AppState, staged: Vec<IssuedEffect>) {
    if let Some(issued) = staged.first() {
        slot.error_message = Some(format!(
            "staged {:?} effect reached a pure apply site and was not executed",
            issued.effect.family()
        ));
    }
}

/// Strict test-support commit: yield the next state of a transition that must
/// stage no effects.
///
/// Tests use this so no staged effect can be dropped silently: it panics when
/// the transition failed to commit **or** when it staged effects. Production
/// paths must destructure the [`Transition`] and execute its effects instead.
pub trait TransitionExt {
    /// Return the committed next state, panicking on error or staged effects.
    fn committed_pure(self) -> AppState;

    /// Return the committed next state, explicitly discarding staged effects.
    ///
    /// Test-only acknowledgment for sites that exercise the state semantics
    /// of an effect-staging message without executing its effects. Production
    /// code must never discard staged effects.
    fn committed_discarding_effects(self) -> AppState;
}

impl TransitionExt for Result<Transition, TransitionError> {
    fn committed_pure(self) -> AppState {
        match self {
            Ok(transition) => {
                assert!(
                    transition.effects.is_empty(),
                    "committed_pure drops staged effects; destructure the Transition instead"
                );
                transition.next_state
            }
            Err(error) => panic!("transition must commit: {error}"),
        }
    }

    fn committed_discarding_effects(self) -> AppState {
        match self {
            Ok(transition) => transition.next_state,
            Err(error) => panic!("transition must commit: {error}"),
        }
    }
}

impl Transition {
    /// Commit a transition, enforcing the ordered-effect bound.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::EffectLimitExceeded`] with the untouched
    /// state when more than [`MAX_TRANSITION_EFFECTS`] effects are supplied.
    pub fn new(next_state: AppState, effects: Vec<IssuedEffect>) -> Result<Self, TransitionError> {
        if effects.len() > MAX_TRANSITION_EFFECTS {
            return Err(TransitionError::EffectLimitExceeded {
                state: Box::new(next_state),
                attempted: effects.len(),
            });
        }
        Ok(Self {
            next_state,
            effects,
        })
    }

    /// Commit a pure transition with no effects.
    #[must_use]
    pub fn pure(next_state: AppState) -> Self {
        Self {
            next_state,
            effects: Vec::new(),
        }
    }
}

/// Rejected transition commit.
#[derive(Debug)]
pub enum TransitionError {
    /// More than [`MAX_TRANSITION_EFFECTS`] ordered effects were supplied.
    /// The untouched next state is returned so no work is lost.
    EffectLimitExceeded {
        state: Box<AppState>,
        attempted: usize,
    },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EffectLimitExceeded { attempted, .. } => write!(
                formatter,
                "transition committed {attempted} effects; the bound is {MAX_TRANSITION_EFFECTS}"
            ),
        }
    }
}

/// One bounded pending effect record stored in [`AppState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEffect {
    pub correlation: Correlation,
    pub retry: RetryPolicy,
}

/// Bounded pending-effect correlation store plus the generation counters that
/// stale completions are checked against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectLedger {
    records: Vec<PendingEffect>,
    next_correlation: u64,
    pub screen_generation: u64,
    pub activation_generation: u64,
    /// Effects staged by reducer handlers during the current message; drained
    /// into the committed [`Transition`] by `apply_message`.
    pub(crate) staged: Vec<IssuedEffect>,
}

impl EffectLedger {
    /// Number of pending records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Drain the staged persistence effect, if one was staged.
    ///
    /// Used by the schedule boundary, which owns the durable write and must
    /// not execute the other families staged in the same transition.
    pub(super) fn take_staged_persist(&mut self) -> Option<IssuedEffect> {
        let index = self
            .staged
            .iter()
            .position(|issued| matches!(issued.effect, Effect::Persistence(_)))?;
        Some(self.staged.remove(index))
    }

    /// Whether no effect is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterate pending records in issue order.
    pub fn iter(&self) -> impl Iterator<Item = &PendingEffect> {
        self.records.iter()
    }

    /// Register a pending effect for `semantic_key`, superseding any older
    /// pending record with the same semantic key.
    ///
    /// # Errors
    ///
    /// Returns [`EffectLedgerError::PendingLimitExceeded`] when the bounded
    /// store is full of distinct semantic keys.
    pub fn register(
        &mut self,
        owner: Id,
        semantic_key: SemanticKey,
        retry: RetryPolicy,
    ) -> Result<Correlation, EffectLedgerError> {
        self.records
            .retain(|record| record.correlation.semantic_key != semantic_key);
        if self.records.len() >= MAX_TRANSITION_EFFECTS {
            return Err(EffectLedgerError::PendingLimitExceeded {
                limit: MAX_TRANSITION_EFFECTS,
            });
        }
        let correlation = Correlation {
            correlation_id: CorrelationId::new(self.next_correlation),
            owner,
            screen_generation: self.screen_generation,
            activation_generation: self.activation_generation,
            semantic_key,
        };
        self.next_correlation = self.next_correlation.wrapping_add(1);
        self.records.push(PendingEffect {
            correlation: correlation.clone(),
            retry,
        });
        Ok(correlation)
    }

    /// Apply a completion identity: remove and report the exact pending match,
    /// or report stale without changing anything.
    pub fn complete(&mut self, correlation: &Correlation) -> CompletionOutcome {
        let matched = self
            .records
            .iter()
            .position(|record| record.correlation.matches(correlation));
        match matched {
            Some(index) => {
                let _ = self.records.remove(index);
                CompletionOutcome::Applied
            }
            None => CompletionOutcome::StaleIgnored,
        }
    }
}

/// Result of checking one completion against the pending ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionOutcome {
    /// The completion matched its pending record exactly and was consumed.
    Applied,
    /// No exact pending match; state must remain byte-equivalent.
    StaleIgnored,
}

/// Rejected pending-effect registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectLedgerError {
    /// The bounded pending store already holds `limit` distinct semantic keys.
    PendingLimitExceeded { limit: usize },
}

impl std::fmt::Display for EffectLedgerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PendingLimitExceeded { limit } => {
                write!(formatter, "pending effect records are bounded at {limit}")
            }
        }
    }
}

impl std::error::Error for EffectLedgerError {}

impl AppState {
    /// Register a pending effect, stage it for the committing transition, and
    /// return its exact correlation identity.
    ///
    /// # Errors
    ///
    /// Returns [`EffectLedgerError::PendingLimitExceeded`] when the bounded
    /// pending store is full of distinct semantic keys.
    pub fn register_pending_effect(
        &mut self,
        owner: Id,
        semantic_key: SemanticKey,
        effect: Effect,
        retry: RetryPolicy,
    ) -> Result<Correlation, EffectLedgerError> {
        let correlation = self.pending_effects.register(owner, semantic_key, retry)?;
        self.pending_effects.staged.push(IssuedEffect {
            effect,
            correlation: correlation.clone(),
            retry,
        });
        Ok(correlation)
    }

    /// Apply a completion identity against the pending ledger.
    ///
    /// A stale or duplicate completion returns
    /// [`CompletionOutcome::StaleIgnored`] and leaves the state untouched.
    pub fn apply_effect_completion(&mut self, correlation: &Correlation) -> CompletionOutcome {
        self.pending_effects.complete(correlation)
    }
}
