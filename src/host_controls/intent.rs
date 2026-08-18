use super::{
    ControlAction, ControlIntent, ControlKind, ErrorBody, FormBody, HostControlRow, Id,
    IntentInput, PanelBody, PanelEvent, PanelHitTarget, PanelSnapshot, TreeBody, TypedMap,
    TypedValue, selected_control_id, selected_tree_id, visible_tree_nodes, wrap_text,
};

pub(super) fn public_control_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    match input.action.clone() {
        ControlAction::Previous => selection_intent(kind, input, false),
        ControlAction::Next => selection_intent(kind, input, true),
        ControlAction::Activate => activation_intent(kind, input),
        ControlAction::Select(id) => select_intent(kind, input.snapshot, &id),
        ControlAction::Action(id) => exact_action_intent(input.snapshot, &id),
        ControlAction::FocusedAction => action_intent(input.snapshot, input.focus_target),
        ControlAction::Submit => submit_intent(kind, input),
        ControlAction::PageNext => page_next_intent(kind, input.snapshot),
        ControlAction::Retry => match (kind, &input.snapshot.body) {
            (ControlKind::Error, PanelBody::Error(body)) if retry_enabled(input.snapshot, body) => {
                ControlIntent::Event(PanelEvent::Retry)
            }
            _ => ControlIntent::None,
        },
        ControlAction::Cancel => match (kind, &input.snapshot.body) {
            (ControlKind::Progress, PanelBody::Progress(body)) if body.cancellable => {
                ControlIntent::Event(PanelEvent::Cancel)
            }
            _ => ControlIntent::None,
        },
        ControlAction::Link(id) => link_intent(kind, input.snapshot, &id),
        ControlAction::FocusedLink => focused_link_intent(kind, input),
    }
}

fn select_intent(kind: ControlKind, snapshot: &PanelSnapshot, id: &Id) -> ControlIntent {
    let exists = match (kind, &snapshot.body) {
        (ControlKind::List, PanelBody::List(body)) => body.items.iter().any(|item| &item.id == id),
        (ControlKind::Tree, PanelBody::Tree(body)) => {
            visible_tree_nodes(body).iter().any(|node| &node.id == id)
        }
        (ControlKind::StructuredDiff, PanelBody::StructuredDiff(body)) => {
            body.files.iter().any(|file| &file.id == id)
        }
        _ => false,
    };
    if exists {
        ControlIntent::Event(PanelEvent::Selected { id: id.clone() })
    } else {
        ControlIntent::None
    }
}

fn exact_action_intent(snapshot: &PanelSnapshot, id: &Id) -> ControlIntent {
    snapshot
        .action_affordances
        .iter()
        .find(|affordance| &affordance.id == id && affordance.enabled)
        .map_or(ControlIntent::None, |affordance| {
            ControlIntent::Event(PanelEvent::Action {
                id: affordance.id.clone(),
                arguments: affordance.arguments.clone().unwrap_or_default(),
            })
        })
}

fn submit_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    match (kind, &input.snapshot.body) {
        (ControlKind::Form, PanelBody::Form(body)) if submit_enabled(input.snapshot, body) => {
            ControlIntent::Event(PanelEvent::Submit {
                values: form_values(body, input.form_draft),
            })
        }
        _ => ControlIntent::None,
    }
}

fn submit_enabled(snapshot: &PanelSnapshot, body: &FormBody) -> bool {
    snapshot
        .action_affordances
        .iter()
        .any(|affordance| affordance.action_id == body.submit_action && affordance.enabled)
}

fn form_values(body: &FormBody, draft: Option<&TypedMap>) -> TypedMap {
    body.fields
        .iter()
        .filter_map(|field| {
            draft
                .and_then(|values| values.get(field.id()))
                .or_else(|| body.values.get(field.id()))
                .map(|value| (field.id().clone(), value.clone()))
        })
        .collect()
}

fn retry_enabled(snapshot: &PanelSnapshot, body: &ErrorBody) -> bool {
    body.retryable
        && body
            .retry_action
            .as_ref()
            .is_some_and(|action| affordance_enabled(snapshot, action))
}

fn page_next_intent(kind: ControlKind, snapshot: &PanelSnapshot) -> ControlIntent {
    match (kind, &snapshot.body) {
        (ControlKind::List, PanelBody::List(body)) => {
            body.next_page_token
                .as_ref()
                .map_or(ControlIntent::None, |token| {
                    ControlIntent::Event(PanelEvent::PageRequested {
                        token: token.clone(),
                    })
                })
        }
        _ => ControlIntent::None,
    }
}

fn link_intent(kind: ControlKind, snapshot: &PanelSnapshot, id: &Id) -> ControlIntent {
    match (kind, &snapshot.body) {
        (ControlKind::Detail, PanelBody::Detail(body))
            if body.actions.contains(id) && affordance_enabled(snapshot, id) =>
        {
            ControlIntent::Event(PanelEvent::LinkSelected {
                link_id: id.clone(),
            })
        }
        _ => ControlIntent::None,
    }
}

fn focused_link_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    let (ControlKind::Detail, PanelBody::Detail(body)) = (kind, &input.snapshot.body) else {
        return ControlIntent::None;
    };
    body.actions
        .iter()
        .find(|id| input.focus_target == Some(*id) && affordance_enabled(input.snapshot, id))
        .or_else(|| {
            body.actions
                .iter()
                .find(|id| affordance_enabled(input.snapshot, id))
        })
        .map_or(ControlIntent::None, |id| {
            ControlIntent::Event(PanelEvent::LinkSelected {
                link_id: id.clone(),
            })
        })
}

fn selection_intent(kind: ControlKind, input: IntentInput<'_>, forward: bool) -> ControlIntent {
    match (kind, &input.snapshot.body) {
        (ControlKind::List, PanelBody::List(body)) => cycle_selection(
            body.items.iter().map(|item| &item.id).collect(),
            selected_control_id(input.snapshot, input.selected_id),
            forward,
        ),
        (ControlKind::Tree, PanelBody::Tree(body)) => {
            let nodes = visible_tree_nodes(body);
            cycle_selection(
                nodes.iter().map(|node| &node.id).collect(),
                selected_control_id(input.snapshot, input.selected_id),
                forward,
            )
        }
        (ControlKind::StructuredDiff, PanelBody::StructuredDiff(body)) => cycle_selection(
            body.files.iter().map(|file| &file.id).collect(),
            selected_control_id(input.snapshot, input.selected_id),
            forward,
        ),
        _ => ControlIntent::Scroll(if forward { 1 } else { -1 }),
    }
}

fn cycle_selection(ids: Vec<&Id>, selected: Option<&Id>, forward: bool) -> ControlIntent {
    if ids.is_empty() {
        return ControlIntent::None;
    }
    let current = selected.and_then(|selected| ids.iter().position(|id| *id == selected));
    let index = if forward {
        current.map_or(0, |index| (index + 1) % ids.len())
    } else {
        current.map_or(ids.len() - 1, |index| {
            index.checked_sub(1).unwrap_or(ids.len() - 1)
        })
    };
    ControlIntent::Event(PanelEvent::Selected {
        id: ids[index].clone(),
    })
}

fn activation_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    match (kind, &input.snapshot.body) {
        (ControlKind::List, PanelBody::List(_))
        | (ControlKind::StructuredDiff, PanelBody::StructuredDiff(_)) => {
            activation_for_id(selected_control_id(input.snapshot, input.selected_id))
        }
        (ControlKind::Tree, PanelBody::Tree(body)) => activate_tree(body, input.selected_id),
        (ControlKind::Form, PanelBody::Form(body)) if submit_enabled(input.snapshot, body) => {
            ControlIntent::Event(PanelEvent::Submit {
                values: form_values(body, input.form_draft),
            })
        }
        (ControlKind::Detail, PanelBody::Detail(body)) => body
            .actions
            .iter()
            .find(|id| input.focus_target == Some(*id) && affordance_enabled(input.snapshot, id))
            .or_else(|| {
                body.actions
                    .iter()
                    .find(|id| affordance_enabled(input.snapshot, id))
            })
            .map_or_else(
                || action_intent(input.snapshot, input.focus_target),
                |link_id| {
                    ControlIntent::Event(PanelEvent::LinkSelected {
                        link_id: link_id.clone(),
                    })
                },
            ),
        (ControlKind::Error, PanelBody::Error(body)) if retry_enabled(input.snapshot, body) => {
            ControlIntent::Event(PanelEvent::Retry)
        }
        (ControlKind::Progress, PanelBody::Progress(body)) if body.cancellable => {
            ControlIntent::Event(PanelEvent::Cancel)
        }
        (ControlKind::Status, PanelBody::Status(_)) | (ControlKind::Empty, PanelBody::Empty(_)) => {
            action_intent(input.snapshot, input.focus_target)
        }
        _ => ControlIntent::None,
    }
}

fn activate_tree(body: &TreeBody, selected_id: Option<&Id>) -> ControlIntent {
    let selected = selected_tree_id(body, selected_id)
        .and_then(|id| body.nodes.iter().find(|node| &node.id == id));
    selected.map_or(ControlIntent::None, |node| {
        if node.expandable {
            ControlIntent::Event(PanelEvent::ExpansionChanged {
                id: node.id.clone(),
                expanded: !node.expanded,
            })
        } else {
            ControlIntent::Event(PanelEvent::Activated {
                id: node.id.clone(),
            })
        }
    })
}

fn activation_for_id(id: Option<&Id>) -> ControlIntent {
    id.map_or(ControlIntent::None, |id| {
        ControlIntent::Event(PanelEvent::Activated { id: id.clone() })
    })
}

fn affordance_enabled(snapshot: &PanelSnapshot, id: &Id) -> bool {
    snapshot
        .action_affordances
        .iter()
        .any(|affordance| &affordance.id == id && affordance.enabled)
}

fn action_intent(snapshot: &PanelSnapshot, focus_target: Option<&Id>) -> ControlIntent {
    snapshot
        .action_affordances
        .iter()
        .find(|affordance| Some(&affordance.id) == focus_target && affordance.enabled)
        .or_else(|| {
            snapshot
                .action_affordances
                .iter()
                .find(|affordance| affordance.enabled)
        })
        .map_or(ControlIntent::None, |affordance| {
            ControlIntent::Event(PanelEvent::Action {
                id: affordance.id.clone(),
                arguments: affordance.arguments.clone().unwrap_or_default(),
            })
        })
}

pub(super) fn push_wrapped(
    rows: &mut Vec<HostControlRow>,
    text: &str,
    width: usize,
    target: Option<PanelHitTarget>,
) {
    for row in wrap_text(text, width) {
        rows.push(HostControlRow {
            text: row.text,
            target: target.clone(),
        });
    }
}

pub(super) fn display_value(value: &TypedValue) -> String {
    match value {
        TypedValue::String(value) => value.clone(),
        TypedValue::Bool(value) => value.to_string(),
        TypedValue::Integer(value) => value.to_string(),
        TypedValue::Decimal(value) => value.as_str().to_owned(),
        TypedValue::Datetime(value) => value.as_str().to_owned(),
        TypedValue::List(values) => values
            .iter()
            .map(display_value)
            .collect::<Vec<_>>()
            .join(", "),
        TypedValue::Map(_) => "<map>".to_owned(),
        TypedValue::SecretRef(reference) => format!("set ({})", reference.env.env()),
    }
}
