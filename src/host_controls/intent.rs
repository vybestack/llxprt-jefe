use super::{
    Affordance, ControlAction, ControlIntent, ControlKind, ErrorBody, FormBody, HostControlRow, Id,
    IntentInput, PanelBody, PanelEvent, PanelHitTarget, TreeBody, TypedMap, TypedValue,
    selected_control_id_body, selected_tree_id, visible_tree_nodes, wrap_text,
};

pub(super) fn public_control_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    match input.action.clone() {
        ControlAction::Previous => selection_intent(kind, input, false),
        ControlAction::Next => selection_intent(kind, input, true),
        ControlAction::Activate => activation_intent(kind, input),
        ControlAction::Select(id) => select_intent(kind, input.body, &id),
        ControlAction::EditField { field_id, value } => {
            edit_field_intent(kind, input.body, field_id, value)
        }
        ControlAction::Action(id) => exact_action_intent(input.action_affordances, &id),
        ControlAction::FocusedAction => action_intent(input.action_affordances, input.focus_target),
        ControlAction::Submit => submit_intent(kind, input),
        ControlAction::PagePrevious => page_previous_intent(kind, input.body),
        ControlAction::PageNext => page_next_intent(kind, input.body),
        ControlAction::Retry => match (kind, input.body) {
            (ControlKind::Error, PanelBody::Error(body))
                if retry_enabled(input.action_affordances, body) =>
            {
                ControlIntent::Event(PanelEvent::Retry)
            }
            _ => ControlIntent::None,
        },
        ControlAction::Cancel => match (kind, input.body) {
            (ControlKind::Progress, PanelBody::Progress(body)) if body.cancellable => {
                ControlIntent::Event(PanelEvent::Cancel)
            }
            _ => ControlIntent::None,
        },
        ControlAction::Link(id) => link_intent(kind, input.body, input.action_affordances, &id),
        ControlAction::FocusedLink => focused_link_intent(kind, input),
    }
}

fn edit_field_intent(
    kind: ControlKind,
    body: &PanelBody,
    field_id: Id,
    value: TypedValue,
) -> ControlIntent {
    let (ControlKind::Form, PanelBody::Form(form)) = (kind, body) else {
        return ControlIntent::None;
    };
    if !form.fields.iter().any(|field| field.id() == &field_id) {
        return ControlIntent::None;
    }
    ControlIntent::Event(PanelEvent::FieldChanged { field_id, value })
}

fn select_intent(kind: ControlKind, body: &PanelBody, id: &Id) -> ControlIntent {
    let exists = match (kind, body) {
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

fn exact_action_intent(affordances: &[Affordance], id: &Id) -> ControlIntent {
    affordances
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
    match (kind, input.body) {
        (ControlKind::Form, PanelBody::Form(body))
            if submit_enabled(input.action_affordances, body) =>
        {
            ControlIntent::Event(PanelEvent::Submit {
                values: form_values(body, input.form_draft),
            })
        }
        _ => ControlIntent::None,
    }
}

fn submit_enabled(affordances: &[Affordance], body: &FormBody) -> bool {
    affordances
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

fn retry_enabled(affordances: &[Affordance], body: &ErrorBody) -> bool {
    body.retryable
        && body
            .retry_action
            .as_ref()
            .is_some_and(|action| affordance_enabled(affordances, action))
}

fn page_next_intent(kind: ControlKind, body: &PanelBody) -> ControlIntent {
    match (kind, body) {
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

/// The List protocol pages forward by token; it cannot name a previous page,
/// so paging back is host-local state the factory defers to the host layer —
/// the same split of authority [`ControlIntent::Scroll`] uses for rows.
fn page_previous_intent(kind: ControlKind, body: &PanelBody) -> ControlIntent {
    match (kind, body) {
        (ControlKind::List, PanelBody::List(body)) if !body.items.is_empty() => {
            ControlIntent::PagePrevious
        }
        _ => ControlIntent::None,
    }
}

fn link_intent(
    kind: ControlKind,
    body: &PanelBody,
    affordances: &[Affordance],
    id: &Id,
) -> ControlIntent {
    match (kind, body) {
        (ControlKind::Detail, PanelBody::Detail(body))
            if body.actions.contains(id) && affordance_enabled(affordances, id) =>
        {
            ControlIntent::Event(PanelEvent::LinkSelected {
                link_id: id.clone(),
            })
        }
        _ => ControlIntent::None,
    }
}

fn focused_link_intent(kind: ControlKind, input: IntentInput<'_>) -> ControlIntent {
    let (ControlKind::Detail, PanelBody::Detail(body)) = (kind, input.body) else {
        return ControlIntent::None;
    };
    body.actions
        .iter()
        .find(|id| {
            input.focus_target == Some(*id) && affordance_enabled(input.action_affordances, id)
        })
        .or_else(|| {
            body.actions
                .iter()
                .find(|id| affordance_enabled(input.action_affordances, id))
        })
        .map_or(ControlIntent::None, |id| {
            ControlIntent::Event(PanelEvent::LinkSelected {
                link_id: id.clone(),
            })
        })
}

fn selection_intent(kind: ControlKind, input: IntentInput<'_>, forward: bool) -> ControlIntent {
    match (kind, input.body) {
        (ControlKind::List, PanelBody::List(body)) => cycle_selection(
            body.items.iter().map(|item| &item.id).collect(),
            selected_control_id_body(input.body, input.selected_id),
            forward,
        ),
        (ControlKind::Tree, PanelBody::Tree(body)) => {
            let nodes = visible_tree_nodes(body);
            cycle_selection(
                nodes.iter().map(|node| &node.id).collect(),
                selected_control_id_body(input.body, input.selected_id),
                forward,
            )
        }
        (ControlKind::StructuredDiff, PanelBody::StructuredDiff(body)) => cycle_selection(
            body.files.iter().map(|file| &file.id).collect(),
            selected_control_id_body(input.body, input.selected_id),
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
    match (kind, input.body) {
        (ControlKind::List, PanelBody::List(_))
        | (ControlKind::StructuredDiff, PanelBody::StructuredDiff(_)) => {
            activation_for_id(selected_control_id_body(input.body, input.selected_id))
        }
        (ControlKind::Tree, PanelBody::Tree(body)) => activate_tree(body, input.selected_id),
        (ControlKind::Form, PanelBody::Form(body))
            if submit_enabled(input.action_affordances, body) =>
        {
            ControlIntent::Event(PanelEvent::Submit {
                values: form_values(body, input.form_draft),
            })
        }
        (ControlKind::Detail, PanelBody::Detail(body)) => body
            .actions
            .iter()
            .find(|id| {
                input.focus_target == Some(*id) && affordance_enabled(input.action_affordances, id)
            })
            .or_else(|| {
                body.actions
                    .iter()
                    .find(|id| affordance_enabled(input.action_affordances, id))
            })
            .map_or_else(
                || action_intent(input.action_affordances, input.focus_target),
                |link_id| {
                    ControlIntent::Event(PanelEvent::LinkSelected {
                        link_id: link_id.clone(),
                    })
                },
            ),
        (ControlKind::Error, PanelBody::Error(body))
            if retry_enabled(input.action_affordances, body) =>
        {
            ControlIntent::Event(PanelEvent::Retry)
        }
        (ControlKind::Progress, PanelBody::Progress(body)) if body.cancellable => {
            ControlIntent::Event(PanelEvent::Cancel)
        }
        (ControlKind::Status, PanelBody::Status(_)) | (ControlKind::Empty, PanelBody::Empty(_)) => {
            action_intent(input.action_affordances, input.focus_target)
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

fn affordance_enabled(affordances: &[Affordance], id: &Id) -> bool {
    affordances
        .iter()
        .any(|affordance| &affordance.id == id && affordance.enabled)
}

fn action_intent(affordances: &[Affordance], focus_target: Option<&Id>) -> ControlIntent {
    affordances
        .iter()
        .find(|affordance| Some(&affordance.id) == focus_target && affordance.enabled)
        .or_else(|| affordances.iter().find(|affordance| affordance.enabled))
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
        rows.push(HostControlRow::new(row.text, target.clone()));
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
