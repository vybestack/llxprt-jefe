//! Runtime-domain reducer handlers plus typed effect-completion application
//! (issue #381 CW01-10/CW01-11).
//!
//! `KillAgent` commits the dead state and stages a bounded
//! [`RuntimeEffect::KillSession`] post-commit effect; the session teardown is
//! executed by the root shell after every state guard is released, never here.

use crate::domain::effects::{
    AgentAvailabilityProbe, Effect, EffectCompletion, EffectFamily, EffectResponse, ProbeEffect,
    ProbeResponse, RetryPolicy, RuntimeEffect, SemanticKey,
};
use crate::domain::{AgentId, AgentStatus, Id};
use crate::messages::RuntimeMessage;

use super::AppState;
use super::transition::CompletionOutcome;

/// Builtin owner recorded on runtime-lifecycle effect correlations.
const RUNTIME_EFFECT_OWNER: &str = "core.dashboard";

impl AppState {
    pub(super) fn apply_runtime_message(&mut self, message: RuntimeMessage) {
        match message {
            RuntimeMessage::KillAgent(agent_id) => self.apply_kill_agent(agent_id),
            RuntimeMessage::AgentStatusChanged(agent_id, status) => {
                self.apply_agent_status_changed(agent_id, status);
            }
            RuntimeMessage::RelaunchAgent(agent_id) => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id)
                    && agent.runtime_binding.is_some()
                {
                    agent.status = AgentStatus::Running;
                    self.sticky_dead_agent_ids.remove(&agent_id);
                    self.clear_dead_preview(&agent_id);
                }
            }
            // RestartAgent handles the edge case where apply_and_persist is
            // called with RestartAgent directly (not via dispatch). The normal
            // path goes through dispatch_restart_agent which applies Kill then
            // Relaunch separately. Here we clear sticky and set Running.
            RuntimeMessage::RestartAgent(agent_id) => {
                self.sticky_dead_agent_ids.remove(&agent_id);
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id)
                    && agent.runtime_binding.is_some()
                {
                    agent.status = AgentStatus::Running;
                    self.clear_dead_preview(&agent_id);
                }
            }
        }
    }

    fn apply_kill_agent(&mut self, agent_id: AgentId) {
        let agent_exists = self.agents.iter().any(|a| a.id == agent_id);
        if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
            agent.status = AgentStatus::Dead;
            agent.runtime_binding = None;
            self.sticky_dead_agent_ids.insert(agent_id.clone());
        }
        // Immediate shell-inventory cleanup on explicit kill (issue #361
        // PR A): the session is being torn down, so any tracked shell window
        // is gone. Natural AgentStatusChanged->Dead is NOT touched here;
        // natural death keeps shell close-only.
        self.remove_shell_window(&agent_id);
        self.clear_dead_preview(&agent_id);
        // The actual session teardown is a bounded post-commit effect
        // (issue #381 CW01-10): staged here, executed by the root shell.
        if agent_exists {
            self.stage_kill_session_effect(agent_id);
        }
    }

    fn apply_agent_status_changed(&mut self, agent_id: AgentId, status: AgentStatus) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.id == agent_id) {
            agent.status = status;
            if status == AgentStatus::Running {
                self.sticky_dead_agent_ids.remove(&agent_id);
                self.clear_dead_preview(&agent_id);
            }
            // Reset scroll state when selected agent's status changes (fix #6).
            if self.selected_agent().is_some_and(|a| a.id == agent_id) {
                self.reset_terminal_scrollback();
            }
        }
    }

    fn stage_kill_session_effect(&mut self, agent_id: AgentId) {
        let Ok(owner) = Id::parse(RUNTIME_EFFECT_OWNER) else {
            self.error_message =
                Some("BUG: builtin runtime effect owner id failed validation".to_owned());
            return;
        };
        let semantic_key = SemanticKey::new(EffectFamily::Runtime, &agent_id.0);
        let effect = Effect::Runtime(RuntimeEffect::KillSession { agent_id });
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    pub(super) fn stage_agent_availability_probes(&mut self, probes: Vec<AgentAvailabilityProbe>) {
        let Ok(owner) = Id::parse(RUNTIME_EFFECT_OWNER) else {
            self.error_message =
                Some("BUG: builtin agent probe effect owner id failed validation".to_owned());
            return;
        };
        for probe in probes {
            let semantic_key =
                SemanticKey::new(EffectFamily::AgentProbe, probe.definition.id.as_str());
            let effect = Effect::AgentProbe(ProbeEffect::CheckAgentAvailability(probe));
            if let Err(error) = self.register_pending_effect(
                owner.clone(),
                semantic_key,
                effect,
                RetryPolicy::Never,
            ) {
                self.error_message = Some(error.to_string());
                return;
            }
        }
    }

    /// Apply a typed post-commit effect completion (issue #381 CW01-11).
    ///
    /// An exact five-field correlation match applies once and clears the
    /// pending record; a stale or duplicate completion leaves the state
    /// untouched. Failure completions surface their redacted detail through
    /// the error channel.
    pub(super) fn apply_effect_completion_message(&mut self, completion: EffectCompletion) {
        match self.apply_effect_completion(&completion.correlation) {
            CompletionOutcome::Applied => match &completion.result {
                Ok(EffectResponse::Persistence(response)) => {
                    self.apply_persistence_response(*response);
                }
                Ok(EffectResponse::AgentProbe(ProbeResponse::Availability {
                    availability,
                    generation,
                })) => {
                    let subject = completion.correlation.semantic_key.subject();
                    let applied = self
                        .agent_type_availability
                        .iter_mut()
                        .find(|observation| observation.type_id().as_str() == subject)
                        .is_some_and(|observation| {
                            observation.apply_probe_result(*generation, *availability.clone())
                        });
                    if applied {
                        let definitions =
                            crate::domain::agent_definition::AgentDefinition::shipped();
                        self.installed_agent_kinds =
                            crate::agent_detection::compatible_legacy_agent_kinds(
                                &self.agent_type_availability,
                                &definitions,
                            );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    self.error_message = Some(format!(
                        "{:?} effect failed: {}",
                        completion.family(),
                        error.redacted_detail
                    ));
                }
            },
            CompletionOutcome::StaleIgnored => {}
        }
    }
}
