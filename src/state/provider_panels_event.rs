//! Pure panel-event validation helpers (issue #391).
//!
//! Private child of [`super::provider_panels`]: stateless free functions that
//! validate a semantic [`PanelEvent`] against a manifest-declared event schema
//! and the currently accepted [`PanelSnapshot`]. No reducer state, I/O, or
//! mutation lives here; every function returns whether the event is admissible.

use super::{EventDeclaration, EventKind};
use crate::domain::plugin::field::Field;
use crate::domain::plugin_config::{validate_field_value, validate_fields};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{FormBody, PanelBody, PanelEvent, PanelSnapshot};

/// Find the declaration matching an event's kind, if declared.
pub(super) fn matching_declaration<'a>(
    allowed: &'a [EventDeclaration],
    event: &PanelEvent,
) -> Option<&'a EventDeclaration> {
    let target = event_kind_of(event);
    allowed
        .iter()
        .find(|declaration| declaration.kind == target)
}

/// The declared kind tag of a semantic event.
pub(super) fn event_kind_of(event: &PanelEvent) -> EventKind {
    match event {
        PanelEvent::Selected { .. } => EventKind::Selected,
        PanelEvent::Activated { .. } => EventKind::Activated,
        PanelEvent::Action { .. } => EventKind::Action,
        PanelEvent::FieldChanged { .. } => EventKind::FieldChanged,
        PanelEvent::Submit { .. } => EventKind::Submit,
        PanelEvent::PageRequested { .. } => EventKind::PageRequested,
        PanelEvent::Retry => EventKind::Retry,
        PanelEvent::Cancel => EventKind::Cancel,
        PanelEvent::LinkSelected { .. } => EventKind::LinkSelected,
        PanelEvent::ExpansionChanged { .. } => EventKind::ExpansionChanged,
    }
}

/// Whether an event references the accepted snapshot correctly.
pub(super) fn validate_event_against_snapshot(
    snapshot: &PanelSnapshot,
    event: &PanelEvent,
    declaration: &EventDeclaration,
) -> bool {
    match event {
        PanelEvent::Selected { id } | PanelEvent::Activated { id } => {
            selection_target_exists(snapshot, id)
        }
        PanelEvent::Action { id, arguments } => action_valid(snapshot, id, arguments, declaration),
        PanelEvent::FieldChanged { field_id, value } => {
            field_value_valid(snapshot, field_id, value)
        }
        PanelEvent::Submit { values } => submit_valid(snapshot, values, declaration),
        PanelEvent::PageRequested { token } => page_token_matches(snapshot, token),
        PanelEvent::Cancel => progress_cancellable(snapshot),
        PanelEvent::LinkSelected { link_id } => link_is_enabled(snapshot, link_id),
        PanelEvent::ExpansionChanged { id, expanded } => {
            tree_expansion_is_valid(snapshot, id, *expanded)
        }
        PanelEvent::Retry => retry_is_enabled(snapshot),
    }
}

fn selection_target_exists(snapshot: &PanelSnapshot, target_id: &Id) -> bool {
    match &snapshot.body {
        PanelBody::List(list) => list.items.iter().any(|item| &item.id == target_id),
        PanelBody::Tree(tree) => tree.nodes.iter().any(|node| &node.id == target_id),
        PanelBody::StructuredDiff(diff) => diff.files.iter().any(|file| &file.id == target_id),
        PanelBody::Detail(_)
        | PanelBody::Form(_)
        | PanelBody::Status(_)
        | PanelBody::Progress(_)
        | PanelBody::Empty(_)
        | PanelBody::Error(_) => false,
    }
}

fn action_valid(
    snapshot: &PanelSnapshot,
    affordance_id: &Id,
    arguments: &TypedMap,
    declaration: &EventDeclaration,
) -> bool {
    let Some(affordance) = snapshot
        .action_affordances
        .iter()
        .find(|candidate| &candidate.id == affordance_id)
    else {
        return false;
    };
    if !affordance.enabled {
        return false;
    }
    arguments_match_declaration(arguments, &declaration.arguments)
}

fn field_value_valid(snapshot: &PanelSnapshot, field_id: &Id, value: &TypedValue) -> bool {
    let PanelBody::Form(form) = &snapshot.body else {
        return false;
    };
    form.fields
        .iter()
        .find(|field| field.id() == field_id)
        .is_some_and(|field| validate_field_value(field, value).is_ok())
}

fn submit_valid(
    snapshot: &PanelSnapshot,
    values: &TypedMap,
    declaration: &EventDeclaration,
) -> bool {
    let PanelBody::Form(form) = &snapshot.body else {
        return false;
    };
    submit_allowed(snapshot, form) && arguments_match_declaration(values, &declaration.arguments)
}

fn submit_allowed(snapshot: &PanelSnapshot, form: &FormBody) -> bool {
    snapshot
        .action_affordances
        .iter()
        .any(|affordance| affordance.enabled && affordance.action_id == form.submit_action)
}

fn arguments_match_declaration(arguments: &TypedMap, declared: &[Field]) -> bool {
    validate_fields(declared, arguments).is_empty()
}

fn page_token_matches(snapshot: &PanelSnapshot, token: &str) -> bool {
    match &snapshot.body {
        PanelBody::List(list) => list.next_page_token.as_deref() == Some(token),
        _ => false,
    }
}

fn link_is_enabled(snapshot: &PanelSnapshot, link_id: &Id) -> bool {
    let PanelBody::Detail(detail) = &snapshot.body else {
        return false;
    };
    detail.actions.iter().any(|action| action == link_id)
        && snapshot
            .action_affordances
            .iter()
            .any(|affordance| affordance.enabled && &affordance.id == link_id)
}

fn retry_is_enabled(snapshot: &PanelSnapshot) -> bool {
    let PanelBody::Error(error) = &snapshot.body else {
        return false;
    };
    error.retryable
        && error.retry_action.as_ref().is_some_and(|retry_action| {
            snapshot
                .action_affordances
                .iter()
                .any(|affordance| affordance.enabled && &affordance.id == retry_action)
        })
}

fn tree_expansion_is_valid(snapshot: &PanelSnapshot, id: &Id, expanded: bool) -> bool {
    let PanelBody::Tree(tree) = &snapshot.body else {
        return false;
    };
    tree.nodes
        .iter()
        .find(|node| &node.id == id)
        .is_some_and(|node| node.expandable && node.expanded != expanded)
}

fn progress_cancellable(snapshot: &PanelSnapshot) -> bool {
    match &snapshot.body {
        PanelBody::Progress(progress) => progress.cancellable,
        _ => false,
    }
}
