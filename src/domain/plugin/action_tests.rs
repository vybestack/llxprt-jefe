//! Action declaration table (issue #389 CW-09, acceptance rows D4 and D5).

use super::*;
use crate::domain::Id;
use crate::domain::plugin::limits::{
    ACTION_ARGUMENT_LIMIT, ACTION_CONTEXT_LIMIT, ACTION_TIMEOUT_SECONDS_LIMIT,
    ACTION_TIMEOUT_SECONDS_MINIMUM,
};
use crate::domain::plugin::{Field, FieldDraft, FieldKind, RestartScope};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn field(name: &str) -> Field {
    Field::parse(FieldDraft {
        id: id(name),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("{name} must parse: {error}"))
}

fn draft() -> ActionDraft {
    ActionDraft {
        id: id("vendor.pkg.run"),
        label: "Run".to_owned(),
        description: "Run the thing".to_owned(),
        category: id("tasks"),
        contexts: vec![id("core.dashboard")],
        arguments: Vec::new(),
        timeout_seconds: 30,
        destructive: false,
        confirmation: ActionConfirmation::None,
        handler: id("run"),
        allowed_outcomes: Vec::new(),
    }
}

fn error_of(draft: ActionDraft) -> ActionError {
    Action::parse(draft)
        .err()
        .unwrap_or_else(|| panic!("the draft must be rejected"))
}

#[test]
fn confirmations_and_outcomes_use_lower_kebab_case_wire_names() {
    assert_eq!(ActionConfirmation::None.as_wire(), "none");
    assert_eq!(
        ActionConfirmation::HostBeforeInvoke.as_wire(),
        "host-before-invoke"
    );
    assert_eq!(
        ActionConfirmation::ProviderContinuation.as_wire(),
        "provider-continuation"
    );
    for value in ActionConfirmation::ALL {
        assert_eq!(ActionConfirmation::from_wire(value.as_wire()), Some(value));
    }

    assert_eq!(
        ActionOutcome::NavigateDeclaredRoute.as_wire(),
        "navigate-declared-route"
    );
    assert_eq!(
        ActionOutcome::RefreshCurrentResource.as_wire(),
        "refresh-current-resource"
    );
    assert_eq!(ActionOutcome::Notice.as_wire(), "notice");
    assert_eq!(
        ActionOutcome::ReplaceOwnedPanel.as_wire(),
        "replace-owned-panel"
    );
    assert_eq!(
        ActionOutcome::RequestHostConfirmation.as_wire(),
        "request-host-confirmation"
    );
    assert_eq!(
        ActionOutcome::CloseOwnedPanel.as_wire(),
        "close-owned-panel"
    );
    for value in ActionOutcome::ALL {
        assert_eq!(ActionOutcome::from_wire(value.as_wire()), Some(value));
    }
}

#[test]
fn the_outcome_set_is_exactly_the_six_declared_kinds() {
    assert_eq!(ActionOutcome::ALL.len(), 6);
}

#[test]
fn a_minimal_action_parses_and_keeps_its_declaration() {
    let action = Action::parse(draft()).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(action.id().as_str(), "vendor.pkg.run");
    assert_eq!(action.label(), "Run");
    assert_eq!(action.timeout_seconds(), 30);
    assert!(!action.destructive());
    assert_eq!(action.confirmation(), ActionConfirmation::None);
    assert!(action.allowed_outcomes().is_empty());
}

#[test]
fn an_action_must_declare_at_least_one_context() {
    let mut candidate = draft();
    candidate.contexts = Vec::new();
    assert_eq!(error_of(candidate), ActionError::NoContexts);
}

#[test]
fn contexts_accept_their_limit_and_reject_one_more() {
    let mut at_limit = draft();
    at_limit.contexts = (0..ACTION_CONTEXT_LIMIT)
        .map(|index| id(&format!("ctx.c{index}")))
        .collect();
    assert!(Action::parse(at_limit).is_ok());

    let mut over_limit = draft();
    over_limit.contexts = (0..=ACTION_CONTEXT_LIMIT)
        .map(|index| id(&format!("ctx.c{index}")))
        .collect();
    assert_eq!(
        error_of(over_limit),
        ActionError::TooManyContexts {
            len: ACTION_CONTEXT_LIMIT + 1
        }
    );
}

#[test]
fn a_duplicate_context_is_rejected() {
    let mut candidate = draft();
    candidate.contexts = vec![id("core.dashboard"), id("core.dashboard")];
    assert_eq!(
        error_of(candidate),
        ActionError::DuplicateContext {
            id: "core.dashboard".to_owned()
        }
    );
}

#[test]
fn arguments_accept_their_limit_and_reject_one_more() {
    let mut at_limit = draft();
    at_limit.arguments = (0..ACTION_ARGUMENT_LIMIT)
        .map(|index| field(&format!("a{index}")))
        .collect();
    assert!(Action::parse(at_limit).is_ok());

    let mut over_limit = draft();
    over_limit.arguments = (0..=ACTION_ARGUMENT_LIMIT)
        .map(|index| field(&format!("a{index}")))
        .collect();
    assert_eq!(
        error_of(over_limit),
        ActionError::TooManyArguments {
            len: ACTION_ARGUMENT_LIMIT + 1
        }
    );
}

#[test]
fn a_duplicate_argument_id_is_rejected() {
    let mut candidate = draft();
    candidate.arguments = vec![field("same"), field("same")];
    assert_eq!(
        error_of(candidate),
        ActionError::DuplicateArgument {
            id: "same".to_owned()
        }
    );
}

#[test]
fn the_timeout_accepts_both_edges_and_rejects_just_outside() {
    for seconds in [ACTION_TIMEOUT_SECONDS_MINIMUM, ACTION_TIMEOUT_SECONDS_LIMIT] {
        let mut candidate = draft();
        candidate.timeout_seconds = seconds;
        assert!(
            Action::parse(candidate).is_ok(),
            "{seconds}s is inside the declared range"
        );
    }
    for seconds in [
        ACTION_TIMEOUT_SECONDS_MINIMUM - 1,
        ACTION_TIMEOUT_SECONDS_LIMIT + 1,
    ] {
        let mut candidate = draft();
        candidate.timeout_seconds = seconds;
        assert_eq!(
            error_of(candidate),
            ActionError::TimeoutOutOfRange { seconds },
            "{seconds}s is outside the declared range"
        );
    }
}

#[test]
fn outcomes_accept_the_full_set_and_reject_a_repeat() {
    let mut all = draft();
    all.allowed_outcomes = ActionOutcome::ALL.to_vec();
    assert!(
        Action::parse(all).is_ok(),
        "declaring every outcome once is the maximum"
    );

    let mut repeated = draft();
    repeated.allowed_outcomes = vec![ActionOutcome::Notice, ActionOutcome::Notice];
    assert_eq!(
        error_of(repeated),
        ActionError::DuplicateOutcome {
            outcome: "notice".to_owned()
        }
    );
}

#[test]
fn a_destructive_action_must_confirm_before_it_runs() {
    let mut candidate = draft();
    candidate.destructive = true;
    candidate.confirmation = ActionConfirmation::None;
    assert_eq!(
        error_of(candidate),
        ActionError::DestructiveWithoutConfirmation
    );

    for confirmation in [
        ActionConfirmation::HostBeforeInvoke,
        ActionConfirmation::ProviderContinuation,
    ] {
        let mut good = draft();
        good.destructive = true;
        good.confirmation = confirmation;
        assert!(Action::parse(good).is_ok());
    }
}

#[test]
fn a_label_and_description_may_not_be_blank() {
    for blank in ["", "   ", "\t"] {
        let mut labelled = draft();
        labelled.label = blank.to_owned();
        assert_eq!(error_of(labelled), ActionError::BlankLabel);

        let mut described = draft();
        described.description = blank.to_owned();
        assert_eq!(error_of(described), ActionError::BlankDescription);
    }
}
