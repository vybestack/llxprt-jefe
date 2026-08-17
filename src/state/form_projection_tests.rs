//! Behavioral tests for the pure form-projection helpers.

#[test]
fn remote_form_agent_type_choices_follow_preference_order() {
    assert_eq!(
        crate::state::effective_agent_type_ids(&[], true),
        vec![
            crate::domain::shipped_agent_type(3),
            crate::domain::shipped_agent_type(1),
            crate::domain::shipped_agent_type(0),
            crate::domain::shipped_agent_type(2),
        ]
    );
}
