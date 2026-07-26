//! Durable-save staging and persistence completion handling (issue #381).
//!
//! `StageSave` projects the committed state into a schema-2 candidate and
//! stages one bounded [`PersistenceEffect::PersistState`]. Writing bytes,
//! fencing on freshness, and renaming are the root shell's responsibility;
//! this module never touches the filesystem.

use crate::domain::effects::{
    Correlation, Effect, EffectFamily, IssuedEffect, PersistenceEffect, PersistenceResponse,
    RetryPolicy, SemanticKey,
};
use crate::domain::{Id, StateV2};

use super::AppState;
use super::durable_projection::to_durable_state;

/// Builtin owner recorded on durable-save effect correlations.
const PERSISTENCE_EFFECT_OWNER: &str = "core.persistence";

/// Semantic subject for the whole-document save, so a newer candidate always
/// supersedes an older pending one.
const DURABLE_STATE_SUBJECT: &str = "state";

impl AppState {
    /// Project the committed state and stage one durable save effect.
    pub(super) fn stage_durable_save(&mut self) {
        let Ok(owner) = Id::parse(PERSISTENCE_EFFECT_OWNER) else {
            self.error_message =
                Some("BUG: builtin persistence effect owner id failed validation".to_owned());
            return;
        };

        let revision = self
            .durable_revision
            .max(self.proposed_revision)
            .saturating_add(1);
        self.proposed_revision = revision;
        let mut candidate = match to_durable_state(self) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.error_message = Some(format!("durable projection failed: {error}"));
                return;
            }
        };
        candidate.revision = revision;

        let semantic_key = SemanticKey::new(EffectFamily::Persistence, DURABLE_STATE_SUBJECT);
        let effect = Effect::Persistence(PersistenceEffect::PersistState {
            candidate: Box::new(candidate),
            revision,
        });
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    /// Stage a durable save and drain it into a schedulable request.
    ///
    /// Returns the projected candidate, its revision, and the correlation the
    /// completion must carry. Returns `None` when projection failed (the error
    /// is already on the state) so a degraded document is never written.
    ///
    /// Callers own the schedule/write boundary; the state layer only decides
    /// *what* would be written.
    #[must_use]
    pub fn take_durable_save_request(&mut self) -> Option<(Box<StateV2>, u64, Correlation)> {
        self.stage_durable_save();
        let issued = self.pending_effects.take_staged_persist()?;
        let IssuedEffect {
            effect,
            correlation,
            ..
        } = issued;
        // `take_staged_persist` only yields `Effect::Persistence`, and
        // destructuring the inner variant keeps this exhaustive: a new
        // persistence effect must decide how it reaches the worker rather than
        // silently returning `None` and stranding its pending ledger record.
        let Effect::Persistence(persistence) = effect else {
            return None;
        };
        match persistence {
            PersistenceEffect::PersistState {
                candidate,
                revision,
            } => Some((candidate, revision, correlation)),
        }
    }

    /// Apply a persistence completion payload after its correlation matched.
    ///
    /// Only an acknowledged write advances the durable revision; a superseded
    /// candidate never became the authority and is not a user-facing error.
    pub(super) fn apply_persistence_response(&mut self, response: PersistenceResponse) {
        match response {
            PersistenceResponse::Persisted { revision } => {
                self.durable_revision = self.durable_revision.max(revision);
                self.error_message = None;
            }
            PersistenceResponse::Superseded { .. } => {}
        }
    }
}
