//! Shared helpers for the RED-first provider request remediation tests.

use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{Outcome, ProgressPayload};

use super::super::{ActionPolicy, InvokeInput, ProviderRequestState};

pub(super) fn owner() -> Id {
    Id::parse("host").unwrap_or_else(|_e| panic!("valid owner id"))
}

pub(super) fn action() -> Id {
    Id::parse("provider.run").unwrap_or_else(|_e| panic!("valid action id"))
}

pub(super) fn screen() -> Id {
    Id::parse("dashboard").unwrap_or_else(|_e| panic!("valid screen id"))
}

pub(super) fn empty_map() -> TypedMap {
    TypedMap::new()
}

pub(super) fn non_empty_map() -> TypedMap {
    let mut map = TypedMap::new();
    map.insert(
        Id::parse("resource.ref").unwrap_or_else(|_e| panic!("valid id")),
        TypedValue::String("issue-42".to_owned()),
    );
    map
}

pub(super) fn policy(
    confirmation: ActionConfirmation,
    outcomes: &[ActionOutcome],
    destructive: bool,
) -> ActionPolicy {
    ActionPolicy::new(confirmation, outcomes.to_vec(), destructive)
}

pub(super) fn default_policy() -> ActionPolicy {
    policy(ActionConfirmation::None, &[ActionOutcome::Notice], false)
}

pub(super) fn continuation_policy() -> ActionPolicy {
    policy(
        ActionConfirmation::ProviderContinuation,
        &[
            ActionOutcome::RequestHostConfirmation,
            ActionOutcome::Notice,
        ],
        false,
    )
}

pub(super) fn destructive_continuation_policy() -> ActionPolicy {
    policy(
        ActionConfirmation::ProviderContinuation,
        &[
            ActionOutcome::RequestHostConfirmation,
            ActionOutcome::Notice,
        ],
        true,
    )
}

pub(super) fn do_invoke(state: &mut ProviderRequestState) -> super::super::InvokeOutcome {
    do_invoke_with(state, &default_policy(), empty_map(), empty_map())
}

pub(super) fn do_invoke_with(
    state: &mut ProviderRequestState,
    policy: &ActionPolicy,
    refs: TypedMap,
    args: TypedMap,
) -> super::super::InvokeOutcome {
    state
        .invoke(InvokeInput {
            owner: &owner(),
            action_id: &action(),
            context_screen: &screen(),
            context_instance: &screen(),
            context_refs: &refs,
            arguments: &args,
            policy,
        })
        .unwrap_or_else(|e| panic!("invoke: {e}"))
}

pub(super) fn progress(seq: u16, completed: Option<u64>, total: Option<u64>) -> ProgressPayload {
    ProgressPayload {
        sequence: seq,
        message: format!("step {seq}"),
        completed,
        total,
    }
}

pub(super) fn notice_outcome() -> Outcome {
    Outcome::Notice {
        severity: crate::runtime::provider::protocol::Severity::Info,
        message: "completed".to_owned(),
    }
}

pub(super) fn confirmation_outcome(conf_id: &str, destructive: bool) -> Outcome {
    Outcome::RequestHostConfirmation {
        confirmation_id: Id::parse(conf_id).unwrap_or_else(|_e| panic!("valid conf id")),
        title: "Confirm Action".to_owned(),
        body: "Are you sure?".to_owned(),
        confirm_label: "Yes, proceed".to_owned(),
        destructive,
        continuation_schema: vec![],
    }
}
