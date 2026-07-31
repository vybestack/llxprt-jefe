//! Synchronous root composition funnel for action availability.
//!
//! This uses the existing serial closed-effect executor and reducer completion
//! path. It performs no provider, runtime, persistence, or UI I/O.

use jefe::domain::effects::{
    Effect, EffectResponse, IssuedEffect, ProviderEffect, ProviderResponse,
};
use jefe::messages::{AppMessage, RepositoryAgentMessage};
use jefe::services::effect_executor::{AdapterExecution, EffectAdapter, run_effects};
use jefe::state::transition::commit_in_place;

use super::AppStateHandle;

struct ActionAvailabilityAdapter;

impl EffectAdapter for ActionAvailabilityAdapter {
    fn execute(&mut self, issued: &IssuedEffect) -> AdapterExecution {
        let result = match &issued.effect {
            Effect::Provider(ProviderEffect::ProjectActionAvailability { entries }) => Ok(
                EffectResponse::Provider(ProviderResponse::ActionAvailability {
                    entries: entries.clone(),
                }),
            ),
            other => Err(jefe::domain::effects::EffectError::new(
                jefe::domain::effects::EffectErrorKind::Unavailable,
                false,
                &format!(
                    "{:?} effects are not wired in action availability composition",
                    other.family()
                ),
            )),
        };
        AdapterExecution::Completed(result)
    }
}

pub fn refresh_action_availability(app_state: &mut AppStateHandle) {
    let effects = {
        let mut state = app_state.write();
        commit_in_place(
            &mut state,
            AppMessage::RepositoryAgent(RepositoryAgentMessage::ProjectActionAvailability),
        )
    };
    if effects.is_empty() {
        return;
    }
    let mut adapter = ActionAvailabilityAdapter;
    run_effects(effects, &mut adapter, |completion| {
        let mut state = app_state.write();
        commit_in_place(
            &mut state,
            AppMessage::EffectCompletion(Box::new(completion)),
        )
    });
}
