//! App-state dispatch for definition-generated form intents.

use super::AppState;
use super::generated_agent_form::GeneratedAgentFormIntent;
use super::types::ModalState;

impl AppState {
    /// Apply a generated-form intent when that form is active.
    ///
    /// Returns true when the generated modal consumed the intent.
    pub(super) fn handle_generated_form_intent(
        &mut self,
        intent: GeneratedAgentFormIntent,
    ) -> bool {
        let ModalState::GeneratedAgent { form, .. } = &mut self.modal else {
            return false;
        };
        form.apply(intent);
        true
    }

    /// Submit the generated agent form.
    ///
    /// Activates the current focus. When Create is focused and enabled, the
    /// validated result is consumed exactly once and routed through the
    /// canonical agent-creation path. Unsupported or invalid Create leaves
    /// all state, runtime, and persistence untouched (zero effects).
    pub(super) fn submit_generated_form(&mut self) -> bool {
        if !self.handle_generated_form_intent(GeneratedAgentFormIntent::Activate) {
            return false;
        }
        self.consume_generated_form_result();
        true
    }
}
