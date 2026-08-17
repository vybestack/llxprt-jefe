//! Behavioral tests for the durable-save reducer arm (issue #381 S9b-2).
//!
//! Staging a save is a bounded post-commit effect: the reducer assigns the
//! candidate revision and stages one `PersistState` effect carrying the
//! projected schema-2 document. Writing bytes is the root shell's job.

use std::path::PathBuf;

use crate::domain::effects::{
    Correlation, Effect, EffectCompletion, EffectError, EffectErrorKind, EffectFamily,
    EffectResponse, PersistenceEffect, PersistenceResponse,
};
use crate::domain::{Agent, AgentId, Repository, RepositoryId};
use crate::messages::{AppMessage, PersistenceMessage};
use crate::state::AppState;
use crate::state::transition::commit_in_place;

trait TestOptionExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        self.unwrap_or_else(|| panic!("{context}"))
    }
}

fn repository() -> Repository {
    Repository::new(
        RepositoryId("repo-a1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "alpha".to_owned(),
        "alpha".to_owned(),
        PathBuf::from("/work/alpha"),
    )
}

fn state_with_one_agent() -> AppState {
    let repository = repository();
    let agent = Agent::new(
        AgentId("agent-a1".to_owned()),
        repository.id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "runner".to_owned(),
        PathBuf::from("/work/alpha/wt1"),
    );
    let mut state = AppState::test_fixture();
    state.repositories = vec![repository];
    state.agents = vec![agent];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state.durable_revision = 4;
    state.rebuild_repository_agent_ids();
    state
}

fn staged_persist(
    effects: &[crate::domain::effects::IssuedEffect],
) -> (&crate::domain::StateV2, u64, Correlation) {
    let issued = effects
        .iter()
        .find(|issued| matches!(issued.effect, Effect::Persistence(_)))
        .value_or_panic("a persistence effect was staged");
    match &issued.effect {
        Effect::Persistence(PersistenceEffect::PersistState {
            candidate,
            revision,
        }) => (candidate, *revision, issued.correlation.clone()),
        other => panic!("expected PersistState, found {other:?}"),
    }
}

#[test]
fn stage_save_stages_one_persist_effect_with_next_revision() {
    let mut state = state_with_one_agent();
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );

    let (candidate, revision, _) = staged_persist(&effects);
    assert_eq!(
        revision, 5,
        "candidate revision is the next durable revision"
    );
    assert_eq!(candidate.revision, 5);
    assert_eq!(candidate.state_schema, 2);
    assert_eq!(candidate.repositories.len(), 1);
    assert_eq!(candidate.agents.len(), 1);
    assert_eq!(
        state.durable_revision, 4,
        "durable revision only advances once the write is acknowledged"
    );
}

#[test]
fn stage_save_uses_persistence_family_and_never_retries() {
    let mut state = state_with_one_agent();
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );

    let issued = effects
        .iter()
        .find(|issued| matches!(issued.effect, Effect::Persistence(_)))
        .value_or_panic("a persistence effect was staged");
    assert_eq!(
        issued.correlation.semantic_key.family(),
        EffectFamily::Persistence
    );
    assert_eq!(issued.retry, crate::domain::effects::RetryPolicy::Never);
}

#[test]
fn repeated_stage_save_supersedes_the_pending_candidate() {
    let mut state = state_with_one_agent();
    let first = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (_, first_revision, first_correlation) = staged_persist(&first);

    state.agents[0].name = "renamed".to_owned();
    let second = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (candidate, second_revision, second_correlation) = staged_persist(&second);

    assert!(second_revision > first_revision);
    assert_ne!(second_correlation, first_correlation);
    assert_eq!(
        state.pending_effects.len(),
        1,
        "the newer candidate supersedes the older pending save"
    );
    assert!(
        candidate
            .agents
            .iter()
            .any(|agent| agent.values.values().any(|value| matches!(
                value,
                crate::domain::TypedValue::String(text) if text == "renamed"
            ))),
        "the staged candidate reflects the committed state"
    );
}

#[test]
fn persisted_completion_advances_durable_revision_and_clears_error() {
    let mut state = state_with_one_agent();
    state.error_message = Some("previous failure".to_owned());
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (_, revision, correlation) = staged_persist(&effects);

    let completion = EffectCompletion {
        correlation,
        result: Ok(EffectResponse::Persistence(
            PersistenceResponse::Persisted { revision },
        )),
    };
    let follow_ups = commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(completion)),
    );

    assert!(follow_ups.is_empty());
    assert_eq!(state.durable_revision, revision);
    assert_eq!(state.error_message, None);
    assert!(state.pending_effects.is_empty());
}

#[test]
fn superseded_completion_leaves_durable_revision_untouched() {
    let mut state = state_with_one_agent();
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (_, revision, correlation) = staged_persist(&effects);

    let completion = EffectCompletion {
        correlation,
        result: Ok(EffectResponse::Persistence(
            PersistenceResponse::Superseded { revision },
        )),
    };
    commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(completion)),
    );

    assert_eq!(
        state.durable_revision, 4,
        "a superseded candidate never became the durable authority"
    );
    assert_eq!(state.error_message, None, "supersede is not a user error");
    assert!(state.pending_effects.is_empty());
}

#[test]
fn failed_persist_completion_surfaces_error_and_keeps_revision() {
    let mut state = state_with_one_agent();
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (_, _, correlation) = staged_persist(&effects);

    let completion = EffectCompletion {
        correlation,
        result: Err(EffectError::new(
            EffectErrorKind::Io,
            false,
            "state directory is read-only",
        )),
    };
    commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(completion)),
    );

    assert_eq!(state.durable_revision, 4);
    let message = state
        .error_message
        .clone()
        .value_or_panic("failure surfaces through the error channel");
    assert!(
        message.contains("state directory is read-only"),
        "unexpected message: {message}"
    );
}

#[test]
fn stale_persist_completion_is_a_no_op() {
    let mut state = state_with_one_agent();
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (_, revision, correlation) = staged_persist(&effects);

    let completion = EffectCompletion {
        correlation: correlation.clone(),
        result: Ok(EffectResponse::Persistence(
            PersistenceResponse::Persisted { revision },
        )),
    };
    commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(completion)),
    );
    let settled_revision = state.durable_revision;
    state.error_message = Some("later unrelated failure".to_owned());

    // Stage a fresh save so an unrelated correlation is pending, then replay
    // the already-completed one carrying a failure. A ledger that does not
    // match on correlation would consume the pending record and surface this
    // error, so the replay must be ignored on identity alone.
    let _ = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );

    let stale_replay = EffectCompletion {
        correlation,
        result: Err(EffectError::new(
            EffectErrorKind::Io,
            false,
            "replayed completion must not surface",
        )),
    };
    commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(stale_replay)),
    );

    assert_eq!(
        state.durable_revision, settled_revision,
        "a replayed correlation must not move the durable revision"
    );
    assert_eq!(
        state.error_message.as_deref(),
        Some("later unrelated failure"),
        "a duplicate completion must not touch unrelated state"
    );
}

#[test]
fn transient_agents_are_absent_from_the_staged_candidate() {
    let mut state = state_with_one_agent();
    let mut transient = Agent::new(
        AgentId("transient-1f".to_owned()),
        RepositoryId("repo-a1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "scratch".to_owned(),
        PathBuf::from("/tmp/scratch"),
    );
    transient.origin = crate::domain::AgentOrigin::Transient;
    state.agents.push(transient);
    state.rebuild_repository_agent_ids();

    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    let (candidate, _, _) = staged_persist(&effects);

    assert_eq!(candidate.agents.len(), 1);
}

/// The durable revision is the acknowledged-write watermark: taking a request
/// (which the worker will attempt) must not advance it on its own, and a save
/// that is superseded in flight must leave it where it was. Only the
/// acknowledged write moves it (issue #381).
#[test]
fn taking_a_save_request_does_not_advance_the_durable_revision() {
    let mut state = state_with_one_agent();
    let settled = state.durable_revision;

    let (_, requested_revision, correlation) = state
        .take_durable_save_request()
        .unwrap_or_else(|| panic!("a durable save request should be staged"));

    assert!(
        requested_revision > settled,
        "the candidate must claim a newer revision than the settled one"
    );
    assert_eq!(
        state.durable_revision, settled,
        "an in-flight request must not advance the acknowledged watermark"
    );

    // The worker reports the write was superseded before it landed.
    let superseded = EffectCompletion {
        correlation,
        result: Ok(EffectResponse::Persistence(
            PersistenceResponse::Superseded {
                revision: requested_revision,
            },
        )),
    };
    let effects = commit_in_place(
        &mut state,
        AppMessage::EffectCompletion(Box::new(superseded)),
    );

    assert!(effects.is_empty());
    assert_eq!(
        state.durable_revision, settled,
        "a superseded candidate never became the authority"
    );
    assert!(
        state.error_message.is_none(),
        "supersession is normal coalescing, not a user-facing failure"
    );
}

/// Superseding a staged save must also discard the superseded effect, not just
/// its pending record. Otherwise the stale candidate still reaches the worker
/// and an older document can overwrite a newer one on disk (issue #381).
#[test]
fn superseding_a_staged_save_discards_the_stale_candidate() {
    let mut state = state_with_one_agent();

    // Stage a save, then stage another before the first is drained.
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );
    assert_eq!(effects.len(), 1, "the first save stages one effect");

    state.hide_idle_repositories = !state.hide_idle_repositories;
    let effects = commit_in_place(
        &mut state,
        AppMessage::Persistence(PersistenceMessage::StageSave),
    );

    assert_eq!(
        effects.len(),
        1,
        "the newer save supersedes the older one instead of queueing both"
    );

    // Two stages inside a single message must also collapse: `staged` is only
    // drained once per message, so a superseded effect left behind here would
    // still reach the worker and could overwrite a newer document.
    state.stage_durable_save();
    state.stage_durable_save();

    assert_eq!(
        state.pending_effects.iter().count(),
        1,
        "the ledger keeps exactly one pending record per semantic key"
    );
    assert_eq!(
        state.pending_effects.staged.len(),
        1,
        "the superseded candidate must be discarded, not left staged"
    );
}

/// #445: an unreadable state document became an empty one, because the empty
/// fallback was projected straight back over the file. The bytes we failed to
/// read are the only copy of whatever they contain, so nothing may replace
/// them until they are understood.
#[test]
fn a_held_durable_read_writes_nothing() {
    let mut state = state_with_one_agent();
    state.durable_read_held = Some("state.json is not valid JSON".to_owned());

    assert!(
        state.take_durable_save_request().is_none(),
        "an unreadable document must not be overwritten by what little was recovered"
    );
}

/// The mirror hazard: holding writes while the read failed must not become
/// never writing at all.
#[test]
fn a_successful_durable_read_still_writes() {
    let mut state = state_with_one_agent();
    assert_eq!(state.durable_read_held, None);

    assert!(
        state.take_durable_save_request().is_some(),
        "a state loaded from a readable document must still persist"
    );
}
