//! Focused tests for modal-handler helpers.

use super::modal_handlers::{focus_terminal_state, generated_back_is_focused};
use jefe::domain::agent_definition::{AgentDefinition, Availability};
use jefe::state::generated_agent_form::{
    GeneratedAgentForm, GeneratedAgentFormFocus, GeneratedAgentFormIntent,
};
use jefe::state::{ModalState, PaneFocus};

#[test]
fn successful_new_agent_submit_focuses_terminal_pane_and_sets_focused() {
    let mut state = crate::test_app_state();
    state.pane_focus = PaneFocus::Repositories;
    state.terminal_focused = false;

    focus_terminal_state(&mut state);

    assert_eq!(state.pane_focus, PaneFocus::Terminal);
    assert!(state.terminal_focused);
}

#[test]
fn generated_form_back_is_recognized_before_submit_validation() {
    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
    let availability = Availability::InstalledCompatible {
        identity: "0.10.0".to_string(),
        generation: 1,
    };
    let mut form = GeneratedAgentForm::from_definition(&definition, &availability)
        .unwrap_or_else(|error| panic!("LLxprt definition must produce a form: {error}"));
    while !matches!(form.focus(), GeneratedAgentFormFocus::Back) {
        form.apply(GeneratedAgentFormIntent::Next);
    }
    let modal = ModalState::GeneratedAgent {
        type_id: Box::new(definition.id),
        form: Box::new(form),
        return_focus: PaneFocus::Repositories,
        return_agent_type_index: 0,
    };

    assert!(generated_back_is_focused(&modal));
    assert!(!generated_back_is_focused(&ModalState::None));
}
