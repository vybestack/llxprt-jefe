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

    pub(super) fn submit_generated_form(&mut self) -> bool {
        self.handle_generated_form_intent(GeneratedAgentFormIntent::Activate)
    }
}
