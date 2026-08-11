use jefe::domain::{Id, TypedValue};
use jefe::provider_panel_view::PanelHitTarget;
use jefe::runtime::provider::protocol::{Affordance, PanelBody, PanelEvent};
use jefe::state::provider_panels::PanelInstanceId;
use jefe::workbench::screen_registry;

use super::action_handlers::BoundaryAction;

pub(super) fn apply(
    action: BoundaryAction,
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
) {
    let staged = {
        let mut state = app_state.write();
        let current = state.nav.current().clone();
        let Ok(registry) = screen_registry() else {
            return;
        };
        if registry
            .panel_binding(current.screen, &current.panel_focus)
            .is_none()
        {
            return;
        }
        let Some(panel) = state
            .provider_panels
            .panel_for_screen(current.id.get(), &current.panel_focus)
        else {
            return;
        };
        if state.provider_panels.accepted_model_is_stale(panel) {
            return;
        }
        let max_scroll_offset = panel_projection(&state, &current.panel_focus)
            .map_or(0, |projection| projection.max_scroll_offset);
        if let Some(event) = event_for_action(action, &state, panel) {
            state.submit_provider_panel_event(panel, event);
        } else if !apply_local_action(action, &mut state, panel, max_scroll_offset) {
            return;
        }
        state.take_staged_effects()
    };
    super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
}

/// Supported mouse input on a host-rendered provider panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPanelMouseAction {
    /// Left-button press.
    Click,
    /// Scroll toward the beginning of the panel.
    ScrollUp,
    /// Scroll toward the end of the panel.
    ScrollDown,
}

/// Apply provider-panel mouse input through the frame-owned projection.
pub fn apply_mouse(
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
    col: u16,
    row: u16,
    action: ProviderPanelMouseAction,
) -> bool {
    let (consumed, staged) = apply_mouse_to_state(&mut app_state.write(), col, row, action);
    if let Some(staged) = staged {
        super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
    }
    consumed
}

fn apply_mouse_to_state(
    state: &mut jefe::state::AppState,
    col: u16,
    row: u16,
    action: ProviderPanelMouseAction,
) -> (bool, Option<Vec<jefe::domain::effects::IssuedEffect>>) {
    let Some(projection) = mouse_projection(state, col, row) else {
        return (false, None);
    };
    let current = state.nav.current().clone();
    let panel = state
        .provider_panels
        .panel_for_screen(current.id.get(), &projection.id);
    if panel.is_some_and(|instance| state.provider_panels.accepted_model_is_stale(instance)) {
        return (false, None);
    }
    match action {
        ProviderPanelMouseAction::ScrollUp | ProviderPanelMouseAction::ScrollDown => {
            let consumed = panel.is_some_and(|instance| {
                scroll_mouse_panel(state, instance, action, projection.max_scroll_offset)
            });
            (consumed, None)
        }
        ProviderPanelMouseAction::Click => apply_mouse_target(
            state,
            panel,
            projection.id,
            hit_target(&projection, col, row),
        ),
    }
}

fn apply_mouse_target(
    state: &mut jefe::state::AppState,
    panel: Option<PanelInstanceId>,
    panel_id: jefe::workbench::PanelId,
    target: Option<jefe::provider_panel_view::PanelHitTarget>,
) -> (bool, Option<Vec<jefe::domain::effects::IssuedEffect>>) {
    if matches!(target, Some(PanelHitTarget::Unavailable)) {
        return (false, None);
    }
    if let Some(PanelHitTarget::Field(field_id)) = target {
        let Some(instance) = panel else {
            return (false, None);
        };
        if !focus_form_field(state, instance, field_id) {
            return (false, None);
        }
        state.nav.current_mut().panel_focus = panel_id;
        return (true, None);
    }
    let Some(target) = target else {
        state.nav.current_mut().panel_focus = panel_id;
        return (true, None);
    };
    let Some(instance) = panel else {
        return (false, None);
    };
    let Some(event) = mouse_event(state, instance, target) else {
        return (false, None);
    };
    if !state.submit_provider_panel_event(instance, event) {
        return (false, None);
    }
    state.nav.current_mut().panel_focus = panel_id;
    (true, Some(state.take_staged_effects()))
}

fn mouse_projection(
    state: &jefe::state::AppState,
    col: u16,
    row: u16,
) -> Option<jefe::provider_panel_view::PanelProjection> {
    let panel_id = state.resolved_layout.as_ref()?.panel_at(col, row)?.id;
    panel_projection(state, &panel_id)
}

fn panel_projection(
    state: &jefe::state::AppState,
    panel_id: &jefe::workbench::PanelId,
) -> Option<jefe::provider_panel_view::PanelProjection> {
    let layout = state.resolved_layout.as_ref()?;
    let registry = screen_registry().ok()?;
    let descriptor = registry.get_identity(state.screen())?;
    jefe::provider_panel_view::project_provider_screen(
        descriptor,
        state.nav.current().id.get(),
        &state.provider_panels,
        layout,
        &state.nav.current().panel_focus,
    )
    .panels
    .into_iter()
    .find(|panel| &panel.id == panel_id)
}

fn hit_target(
    projection: &jefe::provider_panel_view::PanelProjection,
    col: u16,
    row: u16,
) -> Option<jefe::provider_panel_view::PanelHitTarget> {
    if !projection.content.contains(col, row) {
        return None;
    }
    let index = usize::from(row.saturating_sub(projection.content.row));
    projection.hit_targets.get(index).cloned().flatten()
}

fn scroll_mouse_panel(
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    action: ProviderPanelMouseAction,
    max_scroll_offset: u32,
) -> bool {
    let prior = state
        .provider_panels
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    let scroll_offset = match action {
        ProviderPanelMouseAction::ScrollUp => prior.scroll_offset.saturating_sub(1),
        ProviderPanelMouseAction::ScrollDown => {
            prior.scroll_offset.saturating_add(1).min(max_scroll_offset)
        }
        ProviderPanelMouseAction::Click => return false,
    };
    if scroll_offset == prior.scroll_offset {
        return false;
    }
    state
        .provider_panels
        .update_host_local(
            panel,
            jefe::runtime::provider::protocol::HostLocal {
                scroll_offset,
                ..prior
            },
        )
        .is_ok()
}

fn focus_form_field(
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    field_id: Id,
) -> bool {
    let prior = state
        .provider_panels
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    state
        .provider_panels
        .update_host_local(
            panel,
            jefe::runtime::provider::protocol::HostLocal {
                focus_target: Some(field_id),
                ..prior
            },
        )
        .is_ok()
}

fn mouse_event(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    target: jefe::provider_panel_view::PanelHitTarget,
) -> Option<PanelEvent> {
    match target {
        jefe::provider_panel_view::PanelHitTarget::ListItem(id) => {
            if selected_item(state, panel).as_ref() == Some(&id) {
                Some(PanelEvent::Activated { id })
            } else {
                Some(PanelEvent::Selected { id })
            }
        }
        jefe::provider_panel_view::PanelHitTarget::Action(id) => Some(PanelEvent::Action {
            id,
            arguments: jefe::domain::TypedMap::new(),
        }),
        jefe::provider_panel_view::PanelHitTarget::Submit => submit_event(state, panel),
        jefe::provider_panel_view::PanelHitTarget::PageRequested => page_next_event(state, panel),
        jefe::provider_panel_view::PanelHitTarget::Retry => Some(PanelEvent::Retry),
        jefe::provider_panel_view::PanelHitTarget::Cancel => Some(PanelEvent::Cancel),
        jefe::provider_panel_view::PanelHitTarget::Link(link_id) => {
            Some(PanelEvent::LinkSelected { link_id })
        }
        jefe::provider_panel_view::PanelHitTarget::Field(_)
        | jefe::provider_panel_view::PanelHitTarget::Unavailable => None,
    }
}
fn event_for_action(
    action: BoundaryAction,
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<PanelEvent> {
    match action {
        BoundaryAction::ProviderPanelPrevious => select_list_item(state, panel, false),
        BoundaryAction::ProviderPanelNext => select_list_item(state, panel, true),
        BoundaryAction::ProviderPanelActivate => activation_event(state, panel),
        BoundaryAction::ProviderPanelRetry => Some(PanelEvent::Retry),
        BoundaryAction::ProviderPanelCancel => Some(PanelEvent::Cancel),
        BoundaryAction::ProviderPanelAction => action_event(state, panel),
        BoundaryAction::ProviderPanelSubmit => submit_event(state, panel),
        BoundaryAction::ProviderPanelPageNext => page_next_event(state, panel),
        BoundaryAction::ProviderPanelLinkSelect => link_select_event(state, panel),
        _ => None,
    }
}

fn apply_local_action(
    action: BoundaryAction,
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    max_scroll_offset: u32,
) -> bool {
    let direction = match action {
        BoundaryAction::ProviderPanelPrevious => -1_i8,
        BoundaryAction::ProviderPanelNext => 1_i8,
        _ => return false,
    };
    let Some(snapshot) = state.provider_panels.accepted_snapshot(panel) else {
        return false;
    };
    if matches!(snapshot.body, PanelBody::List(_)) {
        return false;
    }
    let prior = state
        .provider_panels
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    let scroll_offset = if direction < 0 {
        prior.scroll_offset.saturating_sub(1)
    } else {
        prior.scroll_offset.saturating_add(1).min(max_scroll_offset)
    };
    let host = jefe::runtime::provider::protocol::HostLocal {
        scroll_offset,
        ..prior
    };
    state.provider_panels.update_host_local(panel, host).is_ok()
}

fn select_list_item(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    forward: bool,
) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let PanelBody::List(body) = &snapshot.body else {
        return None;
    };
    if body.items.is_empty() {
        return None;
    }
    let selected = selected_item(state, panel);
    let current = selected
        .as_ref()
        .and_then(|id| body.items.iter().position(|item| &item.id == id));
    let index = if forward {
        current.map_or(0, |index| (index + 1) % body.items.len())
    } else {
        current.map_or(body.items.len() - 1, |index| {
            if index == 0 {
                body.items.len() - 1
            } else {
                index - 1
            }
        })
    };
    Some(PanelEvent::Selected {
        id: body.items[index].id.clone(),
    })
}

fn activation_event(state: &jefe::state::AppState, panel: PanelInstanceId) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    match snapshot.body {
        PanelBody::List(_) => selected_item(state, panel).map(|id| PanelEvent::Activated { id }),
        PanelBody::Form(_) => submit_event(state, panel),
        PanelBody::Detail(_) => {
            link_select_event(state, panel).or_else(|| action_event(state, panel))
        }
        PanelBody::Error(_) => Some(PanelEvent::Retry),
        PanelBody::Progress(_) => Some(PanelEvent::Cancel),
        PanelBody::Status(_) | PanelBody::Empty(_) => action_event(state, panel),
    }
}

fn selected_item(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<jefe::domain::Id> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let PanelBody::List(body) = &snapshot.body else {
        return None;
    };
    state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.selected_id.as_ref())
        .filter(|selected| body.items.iter().any(|item| &item.id == *selected))
        .cloned()
        .or_else(|| {
            body.selected_id
                .as_ref()
                .filter(|selected| body.items.iter().any(|item| &item.id == *selected))
                .cloned()
        })
        .or_else(|| body.items.first().map(|item| item.id.clone()))
}

/// Construct a PanelEvent::Action for the focused or first enabled affordance.
fn action_event(state: &jefe::state::AppState, panel: PanelInstanceId) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let focus_target = state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.focus_target.as_ref());
    let affordance =
        focused_or_first_enabled_affordance(&snapshot.action_affordances, focus_target)?;
    Some(PanelEvent::Action {
        id: affordance.id.clone(),
        arguments: affordance.arguments.clone().unwrap_or_default(),
    })
}

/// Construct a PanelEvent::Submit from the host-local form draft.
fn submit_event(state: &jefe::state::AppState, panel: PanelInstanceId) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    if !matches!(snapshot.body, PanelBody::Form(_)) {
        return None;
    }
    let values = state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.form_draft.as_ref())
        .cloned()
        .unwrap_or_default();
    Some(PanelEvent::Submit { values })
}

/// Construct a PanelEvent::PageRequested from the list body's next page token.
fn page_next_event(state: &jefe::state::AppState, panel: PanelInstanceId) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let PanelBody::List(body) = &snapshot.body else {
        return None;
    };
    let token = body.next_page_token.clone()?;
    Some(PanelEvent::PageRequested { token })
}

/// Construct a PanelEvent::LinkSelected for the focused or first detail link.
fn link_select_event(state: &jefe::state::AppState, panel: PanelInstanceId) -> Option<PanelEvent> {
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let PanelBody::Detail(detail) = &snapshot.body else {
        return None;
    };
    let focus_target = state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.focus_target.as_ref());
    let link_id = focus_target
        .filter(|id| detail.actions.contains(id))
        .cloned()
        .or_else(|| {
            detail
                .actions
                .iter()
                .find(|action| {
                    snapshot
                        .action_affordances
                        .iter()
                        .any(|a| &a.id == *action && a.enabled)
                })
                .cloned()
        })?;
    Some(PanelEvent::LinkSelected { link_id })
}

/// Find the affordance the user focused, or the first enabled one as a fallback.
fn focused_or_first_enabled_affordance<'a>(
    affordances: &'a [Affordance],
    focus_target: Option<&Id>,
) -> Option<&'a Affordance> {
    if let Some(target) = focus_target
        && let Some(focused) = affordances.iter().find(|a| &a.id == target && a.enabled)
    {
        return Some(focused);
    }
    affordances.iter().find(|a| a.enabled)
}

enum RawKeyMutation {
    Local,
    Event(PanelEvent),
}

/// Apply provider-owned text/focus input before the global action registry.
///
/// Only keys whose meaning depends on the accepted provider model are handled
/// here. Protected/global commands continue through the normal registry path.
pub fn apply_raw_key(
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
    key_event: &iocraft::prelude::KeyEvent,
) -> bool {
    use iocraft::prelude::{KeyCode, KeyModifiers};

    if key_event.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    ) && (key_event.code != KeyCode::Tab || key_event.modifiers != KeyModifiers::CONTROL)
    {
        return false;
    }
    let (handled, staged) = {
        let mut state = app_state.write();
        let current = state.nav.current().clone();
        let Ok(registry) = screen_registry() else {
            return false;
        };
        if registry
            .panel_binding(current.screen, &current.panel_focus)
            .is_none()
        {
            return false;
        }
        let Some(panel) = state
            .provider_panels
            .panel_for_screen(current.id.get(), &current.panel_focus)
        else {
            return false;
        };
        if state.provider_panels.accepted_model_is_stale(panel) {
            return false;
        }
        let mutation =
            if key_event.code == KeyCode::Tab && key_event.modifiers == KeyModifiers::CONTROL {
                cycle_panel_focus(&mut state, registry, true).then_some(RawKeyMutation::Local)
            } else if matches!(key_event.code, KeyCode::Tab | KeyCode::BackTab) {
                let forward = key_event.code == KeyCode::Tab;
                cycle_panel_target(&mut state, panel, forward).then_some(RawKeyMutation::Local)
            } else {
                edit_form_field(&state, panel, key_event)
            };
        match mutation {
            Some(RawKeyMutation::Local) => (true, Vec::new()),
            Some(RawKeyMutation::Event(event)) => {
                state.submit_provider_panel_event(panel, event);
                (true, state.take_staged_effects())
            }
            None => (false, Vec::new()),
        }
    };
    if handled {
        super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
    }
    handled
}

fn cycle_panel_focus(
    state: &mut jefe::state::AppState,
    registry: &jefe::workbench::ScreenRegistry,
    forward: bool,
) -> bool {
    let current = state.nav.current().clone();
    let Some(descriptor) = registry.get_identity(current.screen) else {
        return false;
    };
    let visible = descriptor
        .focus_order
        .iter()
        .filter(|id| {
            state
                .resolved_layout
                .as_ref()
                .and_then(|layout| layout.panel(id))
                .is_some_and(|panel| panel.visible)
        })
        .copied()
        .collect::<Vec<_>>();
    if visible.is_empty() {
        return false;
    }
    let current_index = visible
        .iter()
        .position(|id| id == &current.panel_focus)
        .unwrap_or(0);
    let next = if forward {
        (current_index + 1) % visible.len()
    } else if current_index == 0 {
        visible.len() - 1
    } else {
        current_index - 1
    };
    state.nav.current_mut().panel_focus = visible[next];
    true
}

fn cycle_panel_target(
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    forward: bool,
) -> bool {
    let Some(snapshot) = state.provider_panels.accepted_snapshot(panel) else {
        return false;
    };
    let mut targets = match &snapshot.body {
        PanelBody::Form(form) => form
            .fields
            .iter()
            .map(|field| field.id().clone())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    targets.extend(
        snapshot
            .action_affordances
            .iter()
            .filter(|affordance| affordance.enabled)
            .map(|affordance| affordance.id.clone()),
    );
    if targets.is_empty() {
        return false;
    }
    let prior = state
        .provider_panels
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    let current = prior
        .focus_target
        .as_ref()
        .and_then(|id| targets.iter().position(|target| target == id));
    let index = if forward {
        current.map_or(0, |index| (index + 1) % targets.len())
    } else {
        current.map_or(targets.len() - 1, |index| {
            if index == 0 {
                targets.len() - 1
            } else {
                index - 1
            }
        })
    };
    let host = jefe::runtime::provider::protocol::HostLocal {
        focus_target: Some(targets[index].clone()),
        ..prior
    };
    state.provider_panels.update_host_local(panel, host).is_ok()
}

fn edit_form_field(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    key_event: &iocraft::prelude::KeyEvent,
) -> Option<RawKeyMutation> {
    use iocraft::prelude::{KeyCode, KeyModifiers};
    use jefe::domain::plugin::FieldKind;

    if key_event.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    ) {
        return None;
    }
    if state.provider_panels.accepted_model_is_stale(panel) {
        return None;
    }
    let snapshot = state.provider_panels.accepted_snapshot(panel)?;
    let PanelBody::Form(form) = &snapshot.body else {
        return None;
    };
    let prior = state
        .provider_panels
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    let field = prior
        .focus_target
        .as_ref()
        .and_then(|id| form.fields.iter().find(|field| field.id() == id))
        .or_else(|| form.fields.first())?
        .clone();
    let current = prior
        .form_draft
        .as_ref()
        .and_then(|draft| draft.get(field.id()))
        .or_else(|| form.values.get(field.id()))
        .cloned();
    let value = match (field.kind(), key_event.code) {
        (FieldKind::String | FieldKind::Path, KeyCode::Char(character)) => {
            let mut text = match current {
                Some(TypedValue::String(text)) => text,
                _ => String::new(),
            };
            text.push(character);
            TypedValue::String(text)
        }
        (FieldKind::String | FieldKind::Path, KeyCode::Backspace) => {
            let mut text = match current {
                Some(TypedValue::String(text)) => text,
                _ => String::new(),
            };
            text.pop()?;
            TypedValue::String(text)
        }
        (FieldKind::Boolean, KeyCode::Char(' ')) => {
            let value = matches!(current, Some(TypedValue::Bool(true)));
            TypedValue::Bool(!value)
        }
        _ => return None,
    };
    jefe::domain::plugin_config::validate_field_value(&field, &value)
        .is_ok()
        .then(|| RawKeyMutation::Event(field_change_event(field.id().clone(), value)))
}

/// Construct a PanelEvent::FieldChanged for a form field edit.
#[must_use]
fn field_change_event(field_id: Id, value: TypedValue) -> PanelEvent {
    PanelEvent::FieldChanged { field_id, value }
}

#[cfg(test)]
mod tests {
    use jefe::domain::{Id, TypedMap, TypedValue};
    use jefe::runtime::provider::protocol::{
        BodyKind, HostLocal, ListBody, ListItem, PanelSnapshot,
    };
    use jefe::state::AppState;
    use jefe::state::provider_panels::{AcceptSnapshot, DeclareInput, EventDeclaration, EventKind};
    use jefe::workbench::PanelId;

    use super::*;

    fn list_snapshot(panel: PanelInstanceId, alpha: Id, beta: Id) -> PanelSnapshot {
        PanelSnapshot {
            model_schema: 1,
            panel_instance_id: panel.as_u64(),
            generation: 1,
            revision: 1,
            kind: BodyKind::List,
            title: "List".to_owned(),
            description: None,
            loading: false,
            action_affordances: Vec::new(),
            body: PanelBody::List(ListBody {
                items: vec![
                    ListItem {
                        id: alpha.clone(),
                        label: "Alpha".to_owned(),
                        description: None,
                        status: None,
                        actions: Vec::new(),
                    },
                    ListItem {
                        id: beta,
                        label: "Beta".to_owned(),
                        description: None,
                        status: None,
                        actions: Vec::new(),
                    },
                ],
                selected_id: Some(alpha),
                next_page_token: None,
            }),
        }
    }

    fn active_list() -> (AppState, PanelInstanceId) {
        let owner = Id::parse("vendor.panel").unwrap_or_else(|error| panic!("owner: {error}"));
        let panel_type =
            Id::parse("vendor.panel.list").unwrap_or_else(|error| panic!("panel type: {error}"));
        let panel_id = PanelId::from_static("main");
        let mut state = AppState::default();
        let allowed_events = [
            EventDeclaration {
                kind: EventKind::Selected,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Activated,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Retry,
                arguments: Vec::new(),
            },
        ];
        let declared = state
            .provider_panels
            .declare(DeclareInput {
                owner: &owner,
                panel_id: &panel_id,
                screen_instance_id: 7,
                panel_type: &panel_type,
                activation: &TypedMap::new(),
                allowed_model_kinds: &[BodyKind::List],
                allowed_events: &allowed_events,
                action_authority: &[],
                process_generation: 1,
            })
            .unwrap_or_else(|error| panic!("declare: {error}"));
        state
            .provider_panels
            .activate(declared.instance)
            .unwrap_or_else(|error| panic!("activate: {error}"));
        let alpha = Id::parse("alpha").unwrap_or_else(|error| panic!("alpha: {error}"));
        let beta = Id::parse("beta").unwrap_or_else(|error| panic!("beta: {error}"));
        let snapshot = list_snapshot(declared.instance, alpha, beta);
        state
            .provider_panels
            .accept_snapshot(AcceptSnapshot {
                owner: &owner,
                received_process_generation: 1,
                payload_byte_count: 256,
                elapsed_ms: 0,
                snapshot: &snapshot,
            })
            .unwrap_or_else(|error| panic!("snapshot: {error}"));
        (state, declared.instance)
    }

    fn select_and_commit(
        state: &mut AppState,
        panel: PanelInstanceId,
        forward: bool,
    ) -> Option<PanelEvent> {
        let event = select_list_item(state, panel, forward)?;
        if !state.submit_provider_panel_event(panel, event.clone()) {
            return None;
        }
        Some(event)
    }

    #[test]
    fn next_and_previous_wrap_list_selection_in_host_local_state() {
        let (mut state, panel) = active_list();

        let next = select_and_commit(&mut state, panel, true);
        assert!(matches!(next, Some(PanelEvent::Selected { ref id }) if id.as_str() == "beta"));
        let previous = select_and_commit(&mut state, panel, false);
        assert!(
            matches!(previous, Some(PanelEvent::Selected { ref id }) if id.as_str() == "alpha")
        );
        let wrapped = select_and_commit(&mut state, panel, false);
        assert!(matches!(wrapped, Some(PanelEvent::Selected { ref id }) if id.as_str() == "beta"));
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .and_then(|local| local.selected_id.as_ref())
                .map(Id::as_str),
            Some("beta")
        );
    }

    #[test]
    fn rejected_selected_event_does_not_mutate_host_selection() {
        let (mut state, panel) = active_list();
        let event = select_list_item(&state, panel, true)
            .unwrap_or_else(|| panic!("list selection must produce an event"));
        state
            .provider_panels
            .suspend(panel)
            .unwrap_or_else(|error| panic!("suspend: {error}"));

        assert!(!state.submit_provider_panel_event(panel, event));
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .and_then(|local| local.selected_id.as_ref()),
            None
        );
    }

    #[test]
    fn activate_uses_host_selection_instead_of_provider_default() {
        let (mut state, panel) = active_list();
        let _ = select_and_commit(&mut state, panel, true);

        assert!(matches!(
            selected_item(&state, panel),
            Some(id) if id.as_str() == "beta"
        ));
    }

    #[test]
    fn live_selected_events_stage_ordered_provider_effects() {
        use jefe::domain::effects::{Effect, ProviderEffect};

        let (mut state, panel) = active_list();
        let selected = PanelEvent::Selected {
            id: Id::parse("alpha").unwrap_or_else(|error| panic!("alpha: {error}")),
        };

        state.submit_provider_panel_event(panel, selected.clone());
        let first = state.take_staged_effects();
        state.submit_provider_panel_event(panel, selected);
        let second = state.take_staged_effects();

        assert!(matches!(
            first.as_slice(),
            [issued]
                if matches!(issued.effect, Effect::Provider(ProviderEffect::PanelEvent { .. }))
        ));
        assert!(matches!(
            second.as_slice(),
            [issued]
                if matches!(issued.effect, Effect::Provider(ProviderEffect::PanelEvent { .. }))
        ));
        assert_ne!(first[0].correlation, second[0].correlation);
    }

    #[test]
    fn retry_from_failed_panel_stages_a_fresh_activation() {
        use jefe::domain::effects::{Effect, ProviderEffect};

        let (mut state, panel) = active_list();
        let owner = Id::parse("vendor.panel").unwrap_or_else(|error| panic!("owner: {error}"));
        let invalid = PanelSnapshot {
            model_schema: 1,
            panel_instance_id: panel.as_u64(),
            generation: 1,
            revision: 2,
            kind: BodyKind::List,
            title: "invalid".to_owned(),
            description: None,
            loading: false,
            action_affordances: Vec::new(),
            body: PanelBody::List(ListBody {
                items: Vec::new(),
                selected_id: None,
                next_page_token: None,
            }),
        };
        assert!(
            state
                .provider_panels
                .accept_snapshot(AcceptSnapshot {
                    owner: &owner,
                    received_process_generation: 1,
                    payload_byte_count: 524_289,
                    elapsed_ms: 1,
                    snapshot: &invalid,
                })
                .is_err()
        );

        state.submit_provider_panel_event(panel, PanelEvent::Retry);
        let staged = state.take_staged_effects();

        assert!(matches!(
            staged.as_slice(),
            [issued]
                if matches!(issued.effect, Effect::Provider(ProviderEffect::ActivatePanel { .. }))
        ));
    }

    #[test]
    fn retry_after_the_first_snapshot_fails_stages_a_fresh_activation() {
        use jefe::domain::effects::{Effect, ProviderEffect};

        let (mut state, panel) = active_list();
        let owner = Id::parse("vendor.panel").unwrap_or_else(|error| panic!("owner: {error}"));
        state
            .provider_panels
            .retry(panel)
            .unwrap_or_else(|error| panic!("retry setup: {error}"));
        let mut invalid = list_snapshot(
            panel,
            Id::parse("alpha").unwrap_or_else(|error| panic!("alpha: {error}")),
            Id::parse("beta").unwrap_or_else(|error| panic!("beta: {error}")),
        );
        invalid.generation = 2;
        assert!(
            state
                .provider_panels
                .accept_snapshot(AcceptSnapshot {
                    owner: &owner,
                    received_process_generation: 1,
                    payload_byte_count: 524_289,
                    elapsed_ms: 1,
                    snapshot: &invalid,
                })
                .is_err()
        );
        assert!(state.provider_panels.accepted_snapshot(panel).is_none());

        assert!(state.submit_provider_panel_event(panel, PanelEvent::Retry));
        let staged = state.take_staged_effects();
        assert!(matches!(
            staged.as_slice(),
            [issued]
                if matches!(issued.effect, Effect::Provider(ProviderEffect::ActivatePanel { .. }))
        ));
    }

    #[test]
    fn oversized_host_selection_stages_no_provider_event_and_preserves_prior_local_state() {
        let (mut state, panel) = active_list();
        let field = Id::parse("draft").unwrap_or_else(|error| panic!("field: {error}"));
        let mut accepted = false;
        for length in (0..=jefe::state::provider_panels::HOST_LOCAL_MAX_BYTES).rev() {
            let mut form_draft = TypedMap::new();
            form_draft.insert(field.clone(), TypedValue::String("x".repeat(length)));
            let host = HostLocal {
                focus_target: None,
                scroll_offset: 0,
                selected_id: None,
                form_draft: Some(form_draft),
            };
            if state.provider_panels.update_host_local(panel, host).is_ok() {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "a largest valid host-local fixture must be found");
        let prior = state
            .provider_panels
            .host_local(panel)
            .cloned()
            .unwrap_or_else(|| panic!("host-local fixture must be retained"));
        let selected = PanelEvent::Selected {
            id: Id::parse("beta").unwrap_or_else(|error| panic!("beta: {error}")),
        };

        assert!(!state.submit_provider_panel_event(panel, selected));
        assert_eq!(state.provider_panels.host_local(panel), Some(&prior));
        assert!(state.take_staged_effects().is_empty());
    }

    // ── Helpers for the remaining five PanelEvent kinds ───────────────────

    fn id(value: &str) -> Id {
        Id::parse(value).unwrap_or_else(|error| panic!("id {value}: {error}"))
    }

    fn action_id(value: &str) -> jefe::domain::action_registry::ActionId {
        jefe::domain::action_registry::ActionId::parse(value)
            .unwrap_or_else(|error| panic!("action id {value}: {error}"))
    }

    fn affordance(affordance_id: &str, action: &str, enabled: bool) -> Affordance {
        Affordance {
            id: id(affordance_id),
            label: affordance_id.to_owned(),
            action_id: action_id(action),
            arguments: None,
            enabled,
            unavailable_reason: if enabled {
                None
            } else {
                Some("busy".to_owned())
            },
        }
    }

    fn snapshot_with_body(
        panel: PanelInstanceId,
        body: PanelBody,
        kind: BodyKind,
        affordances: Vec<Affordance>,
    ) -> PanelSnapshot {
        PanelSnapshot {
            model_schema: 1,
            panel_instance_id: panel.as_u64(),
            generation: 1,
            revision: 1,
            kind,
            title: "Panel".to_owned(),
            description: None,
            loading: false,
            action_affordances: affordances,
            body,
        }
    }

    fn string_field(
        field_id: &str,
        label: &str,
        max: Option<jefe::domain::plugin::field::Scalar>,
    ) -> jefe::domain::plugin::field::Field {
        jefe::domain::plugin::field::Field::parse(jefe::domain::plugin::field::FieldDraft {
            id: id(field_id),
            label: label.to_owned(),
            description: None,
            kind: jefe::domain::plugin::field::FieldKind::String,
            required: false,
            default: None,
            min: None,
            max,
            choices: Vec::new(),
            unique: false,
            visible_when: None,
            restart: jefe::domain::plugin::field::RestartScope::None,
        })
        .unwrap_or_else(|error| panic!("field {field_id}: {error}"))
    }

    fn active_form(
        max: Option<jefe::domain::plugin::field::Scalar>,
    ) -> (AppState, PanelInstanceId) {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.form");
        let submit = action_id("vendor.submit");
        let name = string_field("name", "Name", max);
        let region = string_field("region", "Region", None);
        let mut values = TypedMap::new();
        values.insert(id("name"), TypedValue::String("old".to_owned()));
        values.insert(id("region"), TypedValue::String("us".to_owned()));
        let body = PanelBody::Form(jefe::runtime::provider::protocol::FormBody {
            fields: vec![name, region],
            values,
            field_errors: Vec::new(),
            submit_action: submit.clone(),
        });
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Form,
            vec![affordance("submit", "vendor.submit", true)],
        );
        let mut state = AppState::default();
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Form],
            &all_event_kinds(),
            &[submit],
            &snapshot,
        );
        (state, panel)
    }

    fn assert_mouse_stages_one(
        state: &mut AppState,
        panel: PanelInstanceId,
        target: PanelHitTarget,
    ) {
        let (consumed, staged) = apply_mouse_target(
            state,
            Some(panel),
            PanelId::from_static("main"),
            Some(target),
        );
        assert!(consumed);
        assert_eq!(staged.as_ref().map(Vec::len), Some(1));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn mouse_list_click_selects_then_activates_with_one_effect_each() {
        let (mut state, panel) = active_list();
        let panel_id = PanelId::from_static("main");
        let beta = id("beta");

        let (selected, selected_effects) = apply_mouse_target(
            &mut state,
            Some(panel),
            panel_id,
            Some(PanelHitTarget::ListItem(beta.clone())),
        );
        assert!(selected);
        assert_eq!(selected_effects.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .and_then(|local| local.selected_id.as_ref()),
            Some(&beta)
        );

        let (activated, activated_effects) = apply_mouse_target(
            &mut state,
            Some(panel),
            panel_id,
            Some(PanelHitTarget::ListItem(beta)),
        );
        assert!(activated);
        assert_eq!(activated_effects.as_ref().map(Vec::len), Some(1));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn mouse_form_field_focus_is_host_local_and_effect_free() {
        let (mut state, panel) = active_form(None);
        let panel_id = PanelId::from_static("main");

        let (consumed, staged) = apply_mouse_target(
            &mut state,
            Some(panel),
            panel_id,
            Some(PanelHitTarget::Field(id("region"))),
        );

        assert!(consumed);
        assert!(staged.is_none());
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .and_then(|local| local.focus_target.as_ref())
                .map(Id::as_str),
            Some("region")
        );
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn unavailable_mouse_target_preserves_focus_error_and_effects() {
        let (mut state, panel) = active_list();
        let prior_focus = state.nav.current().panel_focus;
        let prior_local = state.provider_panels.host_local(panel).cloned();
        state.error_message = Some("existing error".to_owned());

        let (consumed, staged) = apply_mouse_target(
            &mut state,
            Some(panel),
            PanelId::from_static("other"),
            Some(PanelHitTarget::Unavailable),
        );

        assert!(!consumed);
        assert!(staged.is_none());
        assert_eq!(state.nav.current().panel_focus, prior_focus);
        assert_eq!(
            state.provider_panels.host_local(panel),
            prior_local.as_ref()
        );
        assert_eq!(state.error_message.as_deref(), Some("existing error"));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn stale_mouse_target_preserves_focus_error_local_state_and_effects() {
        let (mut state, panel) = active_list();
        let prior_focus = state.nav.current().panel_focus;
        let prior_local = state.provider_panels.host_local(panel).cloned();
        state.error_message = Some("existing error".to_owned());
        state
            .provider_panels
            .fail_runtime(panel)
            .unwrap_or_else(|error| panic!("fail panel: {error}"));

        let (consumed, staged) = apply_mouse_target(
            &mut state,
            Some(panel),
            PanelId::from_static("other"),
            Some(PanelHitTarget::ListItem(id("beta"))),
        );

        assert!(!consumed);
        assert!(staged.is_none());
        assert_eq!(state.nav.current().panel_focus, prior_focus);
        assert_eq!(
            state.provider_panels.host_local(panel),
            prior_local.as_ref()
        );
        assert_eq!(state.error_message.as_deref(), Some("existing error"));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn mouse_targets_map_to_the_closed_semantic_event_vocabulary() {
        let (state, panel) = active_list();
        let action = id("open");
        let link = id("details");

        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::Action(action.clone())),
            Some(PanelEvent::Action {
                id: action,
                arguments: TypedMap::new(),
            })
        );
        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::Retry),
            Some(PanelEvent::Retry)
        );
        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::Cancel),
            Some(PanelEvent::Cancel)
        );
        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::Link(link.clone())),
            Some(PanelEvent::LinkSelected { link_id: link })
        );
        assert!(
            mouse_event(&state, panel, PanelHitTarget::Unavailable).is_none(),
            "disabled controls must not infer an event"
        );
    }

    #[test]
    fn mouse_wheel_clamps_to_projection_scroll_bounds_without_effects() {
        let (mut state, panel) = active_list();

        assert!(scroll_mouse_panel(
            &mut state,
            panel,
            ProviderPanelMouseAction::ScrollDown,
            1,
        ));
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .map(|local| local.scroll_offset),
            Some(1)
        );
        assert!(!scroll_mouse_panel(
            &mut state,
            panel,
            ProviderPanelMouseAction::ScrollDown,
            1,
        ));
        assert!(scroll_mouse_panel(
            &mut state,
            panel,
            ProviderPanelMouseAction::ScrollUp,
            1,
        ));
        assert_eq!(
            state
                .provider_panels
                .host_local(panel)
                .map(|local| local.scroll_offset),
            Some(0)
        );
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn retry_and_cancel_mouse_targets_run_their_live_reducers() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.operation");

        let mut retry_state = AppState::default();
        let retry_action = action_id("vendor.retry");
        let retry_snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            PanelBody::Error(jefe::runtime::provider::protocol::ErrorBody {
                code: "temporary".to_owned(),
                message: "Try again".to_owned(),
                retryable: true,
                retry_action: Some(id("retry")),
            }),
            BodyKind::Error,
            vec![affordance("retry", "vendor.retry", true)],
        );
        let retry_panel = declare_and_accept(
            &mut retry_state,
            (&owner, &panel_type),
            &[BodyKind::Error],
            &all_event_kinds(),
            &[retry_action],
            &retry_snapshot,
        );
        assert_mouse_stages_one(&mut retry_state, retry_panel, PanelHitTarget::Retry);

        let mut cancel_state = AppState::default();
        let cancel_snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            PanelBody::Progress(jefe::runtime::provider::protocol::ProgressBody {
                message: "Working".to_owned(),
                completed: Some(1),
                total: Some(2),
                cancellable: true,
            }),
            BodyKind::Progress,
            Vec::new(),
        );
        let cancel_panel = declare_and_accept(
            &mut cancel_state,
            (&owner, &panel_type),
            &[BodyKind::Progress],
            &all_event_kinds(),
            &[],
            &cancel_snapshot,
        );
        assert_mouse_stages_one(&mut cancel_state, cancel_panel, PanelHitTarget::Cancel);
    }

    fn declare_and_accept(
        state: &mut AppState,
        identity: (&Id, &Id),
        kinds: &[BodyKind],
        events: &[EventDeclaration],
        authority: &[jefe::domain::action_registry::ActionId],
        snapshot: &PanelSnapshot,
    ) -> PanelInstanceId {
        let panel_id = PanelId::from_static("main");
        let declared = state
            .provider_panels
            .declare(DeclareInput {
                owner: identity.0,
                panel_id: &panel_id,
                screen_instance_id: 7,
                panel_type: identity.1,
                activation: &TypedMap::new(),
                allowed_model_kinds: kinds,
                allowed_events: events,
                action_authority: authority,
                process_generation: 1,
            })
            .unwrap_or_else(|error| panic!("declare: {error}"));
        state
            .provider_panels
            .activate(declared.instance)
            .unwrap_or_else(|error| panic!("activate: {error}"));
        state
            .provider_panels
            .accept_snapshot(AcceptSnapshot {
                owner: identity.0,
                received_process_generation: 1,
                payload_byte_count: 256,
                elapsed_ms: 0,
                snapshot,
            })
            .unwrap_or_else(|error| panic!("snapshot: {error}"));
        declared.instance
    }

    fn all_event_kinds() -> Vec<EventDeclaration> {
        vec![
            EventDeclaration {
                kind: EventKind::Selected,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Activated,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Action,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::FieldChanged,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Submit,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::PageRequested,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Retry,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::Cancel,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::LinkSelected,
                arguments: Vec::new(),
            },
        ]
    }

    // ── Action event tests ───────────────────────────────────────────────

    #[test]
    fn action_event_targets_first_enabled_affordance() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Empty(jefe::runtime::provider::protocol::EmptyBody {
            message: "Ready".to_owned(),
            action: None,
        });
        let affordances = vec![
            affordance("disabled-action", "vendor.disabled", false),
            affordance("open-action", "vendor.open", true),
        ];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Empty,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Empty],
            &all_event_kinds(),
            &[action_id("vendor.disabled"), action_id("vendor.open")],
            &snapshot,
        );

        let event = action_event(&state, panel);
        assert!(matches!(
            event,
            Some(PanelEvent::Action { ref id, .. }) if id.as_str() == "open-action"
        ));
        assert_mouse_stages_one(&mut state, panel, PanelHitTarget::Action(id("open-action")));
    }

    #[test]
    fn action_event_uses_host_local_focus_when_set() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Empty(jefe::runtime::provider::protocol::EmptyBody {
            message: "Ready".to_owned(),
            action: None,
        });
        let affordances = vec![
            affordance("open-action", "vendor.open", true),
            affordance("delete-action", "vendor.delete", true),
        ];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Empty,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Empty],
            &all_event_kinds(),
            &[action_id("vendor.open"), action_id("vendor.delete")],
            &snapshot,
        );
        let host = HostLocal {
            focus_target: Some(id("delete-action")),
            scroll_offset: 0,
            selected_id: None,
            form_draft: None,
        };
        state
            .provider_panels
            .update_host_local(panel, host)
            .unwrap_or_else(|error| panic!("host-local fixture: {error}"));

        let event = action_event(&state, panel);
        assert!(matches!(
            event,
            Some(PanelEvent::Action { ref id, .. }) if id.as_str() == "delete-action"
        ));
    }

    #[test]
    fn action_event_returns_none_when_no_enabled_affordance() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Empty(jefe::runtime::provider::protocol::EmptyBody {
            message: "Ready".to_owned(),
            action: None,
        });
        let affordances = vec![affordance("disabled-action", "vendor.disabled", false)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Empty,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Empty],
            &all_event_kinds(),
            &[action_id("vendor.disabled")],
            &snapshot,
        );

        assert!(action_event(&state, panel).is_none());
    }

    // ── Submit event tests ───────────────────────────────────────────────

    #[test]
    fn submit_event_uses_host_local_form_draft() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.form");
        let mut state = AppState::default();
        let submit = action_id("vendor.submit");
        let body = PanelBody::Form(jefe::runtime::provider::protocol::FormBody {
            fields: vec![string_field("name", "Name", None)],
            values: TypedMap::new(),
            field_errors: Vec::new(),
            submit_action: submit.clone(),
        });
        let affordances = vec![affordance("submit", "vendor.submit", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Form,
            affordances,
        );
        let mut events = all_event_kinds();
        let Some(submit_declaration) = events
            .iter_mut()
            .find(|declaration| declaration.kind == EventKind::Submit)
        else {
            panic!("submit declaration fixture");
        };
        submit_declaration.arguments = vec![string_field("name", "Name", None)];
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Form],
            &events,
            &[submit],
            &snapshot,
        );
        let mut draft = TypedMap::new();
        draft.insert(id("name"), TypedValue::String("test".to_owned()));
        let host = HostLocal {
            focus_target: None,
            scroll_offset: 0,
            selected_id: None,
            form_draft: Some(draft),
        };
        state
            .provider_panels
            .update_host_local(panel, host)
            .unwrap_or_else(|error| panic!("host-local fixture: {error}"));

        let event = submit_event(&state, panel);
        assert!(matches!(event, Some(PanelEvent::Submit { .. })));
        assert_mouse_stages_one(&mut state, panel, PanelHitTarget::Submit);
    }

    #[test]
    fn submit_event_returns_none_for_non_form_body() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.list");
        let mut state = AppState::default();
        let alpha = id("alpha");
        let beta = id("beta");
        let snapshot = list_snapshot(PanelInstanceId::from_u64(1), alpha, beta);
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::List],
            &all_event_kinds(),
            &[],
            &snapshot,
        );

        assert!(submit_event(&state, panel).is_none());
    }

    // ── PageRequested event tests ────────────────────────────────────────

    #[test]
    fn page_next_event_uses_snapshot_token() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.list");
        let mut state = AppState::default();
        let alpha = id("alpha");
        let body = PanelBody::List(ListBody {
            items: vec![ListItem {
                id: alpha,
                label: "Alpha".to_owned(),
                description: None,
                status: None,
                actions: Vec::new(),
            }],
            selected_id: None,
            next_page_token: Some("page2".to_owned()),
        });
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::List,
            Vec::new(),
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::List],
            &all_event_kinds(),
            &[],
            &snapshot,
        );

        let event = page_next_event(&state, panel);
        assert!(matches!(
            event,
            Some(PanelEvent::PageRequested { ref token }) if token == "page2"
        ));
        assert_mouse_stages_one(&mut state, panel, PanelHitTarget::PageRequested);
    }

    #[test]
    fn page_next_event_returns_none_without_token() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.list");
        let mut state = AppState::default();
        let alpha = id("alpha");
        let beta = id("beta");
        let snapshot = list_snapshot(PanelInstanceId::from_u64(1), alpha, beta);
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::List],
            &all_event_kinds(),
            &[],
            &snapshot,
        );

        assert!(page_next_event(&state, panel).is_none());
    }

    // ── LinkSelected event tests ─────────────────────────────────────────

    #[test]
    fn link_select_event_targets_first_detail_link() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Detail(jefe::runtime::provider::protocol::DetailBody {
            document: "Doc".to_owned(),
            metadata: Vec::new(),
            actions: vec![id("edit-link")],
        });
        let affordances = vec![affordance("edit-link", "vendor.edit", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Detail,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Detail],
            &all_event_kinds(),
            &[action_id("vendor.edit")],
            &snapshot,
        );

        let event = link_select_event(&state, panel);
        assert!(matches!(
            event,
            Some(PanelEvent::LinkSelected { ref link_id }) if link_id.as_str() == "edit-link"
        ));
        assert_mouse_stages_one(&mut state, panel, PanelHitTarget::Link(id("edit-link")));
    }

    #[test]
    fn link_select_event_returns_none_for_non_detail_body() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.list");
        let mut state = AppState::default();
        let alpha = id("alpha");
        let beta = id("beta");
        let snapshot = list_snapshot(PanelInstanceId::from_u64(1), alpha, beta);
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::List],
            &all_event_kinds(),
            &[],
            &snapshot,
        );

        assert!(link_select_event(&state, panel).is_none());
    }

    // ── Stale/invalid zero-effect tests ──────────────────────────────────

    #[test]
    fn action_event_on_suspended_panel_stages_no_provider_effect() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Empty(jefe::runtime::provider::protocol::EmptyBody {
            message: "Ready".to_owned(),
            action: None,
        });
        let affordances = vec![affordance("open-action", "vendor.open", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Empty,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Empty],
            &all_event_kinds(),
            &[action_id("vendor.open")],
            &snapshot,
        );
        state
            .provider_panels
            .suspend(panel)
            .unwrap_or_else(|error| panic!("suspend: {error}"));

        // Suspend drops the model, so action_event has no snapshot to read
        // and correctly produces nothing.
        assert!(action_event(&state, panel).is_none());
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn undeclared_action_kind_stages_no_provider_effect() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.detail");
        let mut state = AppState::default();
        let body = PanelBody::Empty(jefe::runtime::provider::protocol::EmptyBody {
            message: "Ready".to_owned(),
            action: None,
        });
        let affordances = vec![affordance("open-action", "vendor.open", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Empty,
            affordances,
        );
        // Declare every kind except Action.
        let mut events = all_event_kinds();
        events.retain(|e| e.kind != EventKind::Action);
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Empty],
            &events,
            &[action_id("vendor.open")],
            &snapshot,
        );

        let prior_host = state.provider_panels.host_local(panel).cloned();
        state.error_message = Some("existing error".to_owned());
        let Some(event) = action_event(&state, panel) else {
            panic!("enabled action must project an event");
        };
        assert!(!state.submit_provider_panel_event(panel, event));
        assert!(state.take_staged_effects().is_empty());
        assert_eq!(state.provider_panels.host_local(panel), prior_host.as_ref());
        assert_eq!(state.error_message.as_deref(), Some("existing error"));
    }

    // ── FieldChanged event tests ─────────────────────────────────────────

    #[test]
    fn field_change_event_carries_field_id_and_value() {
        let event = field_change_event(id("name"), TypedValue::String("hello".to_owned()));
        assert!(matches!(
            event,
            PanelEvent::FieldChanged { ref field_id, .. } if field_id.as_str() == "name"
        ));
    }

    #[test]
    fn field_change_event_validates_against_form_snapshot() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.form");
        let mut state = AppState::default();
        let submit = action_id("vendor.submit");
        let field =
            jefe::domain::plugin::field::Field::parse(jefe::domain::plugin::field::FieldDraft {
                id: id("name"),
                label: "Name".to_owned(),
                description: None,
                kind: jefe::domain::plugin::field::FieldKind::String,
                required: false,
                default: None,
                min: None,
                max: None,
                choices: Vec::new(),
                unique: false,
                visible_when: None,
                restart: jefe::domain::plugin::field::RestartScope::None,
            })
            .unwrap_or_else(|error| panic!("field: {error}"));
        let body = PanelBody::Form(jefe::runtime::provider::protocol::FormBody {
            fields: vec![field],
            values: TypedMap::new(),
            field_errors: Vec::new(),
            submit_action: submit.clone(),
        });
        let affordances = vec![affordance("submit", "vendor.submit", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Form,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Form],
            &all_event_kinds(),
            &[submit],
            &snapshot,
        );

        let event = field_change_event(id("name"), TypedValue::String("test".to_owned()));
        assert!(state.submit_provider_panel_event(panel, event));
        assert!(!state.take_staged_effects().is_empty());
    }

    #[test]
    fn field_change_event_for_unknown_field_stages_no_effect() {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.form");
        let mut state = AppState::default();
        let submit = action_id("vendor.submit");
        let body = PanelBody::Form(jefe::runtime::provider::protocol::FormBody {
            fields: Vec::new(),
            values: TypedMap::new(),
            field_errors: Vec::new(),
            submit_action: submit.clone(),
        });
        let affordances = vec![affordance("submit", "vendor.submit", true)];
        let snapshot = snapshot_with_body(
            PanelInstanceId::from_u64(1),
            body,
            BodyKind::Form,
            affordances,
        );
        let panel = declare_and_accept(
            &mut state,
            (&owner, &panel_type),
            &[BodyKind::Form],
            &all_event_kinds(),
            &[submit],
            &snapshot,
        );

        let event = field_change_event(id("nonexistent"), TypedValue::String("x".to_owned()));
        assert!(!state.submit_provider_panel_event(panel, event));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn raw_form_edit_preserves_existing_values_and_stages_exactly_one_event() {
        use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};

        let (mut state, panel) = active_form(None);
        let key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x'));
        let mutation = edit_form_field(&state, panel, &key)
            .unwrap_or_else(|| panic!("valid edit must produce a field event"));
        let RawKeyMutation::Event(event) = mutation else {
            panic!("field edit must be semantic");
        };
        assert!(state.provider_panels.host_local(panel).is_none());
        assert!(state.submit_provider_panel_event(panel, event));
        let effects = state.take_staged_effects();
        assert_eq!(effects.len(), 1);
        let draft = state
            .provider_panels
            .host_local(panel)
            .and_then(|host| host.form_draft.as_ref())
            .unwrap_or_else(|| panic!("accepted edit must create a draft"));
        assert_eq!(
            draft.get(&id("name")),
            Some(&TypedValue::String("oldx".to_owned()))
        );
        assert_eq!(
            draft.get(&id("region")),
            Some(&TypedValue::String("us".to_owned()))
        );
    }

    #[test]
    fn invalid_raw_form_edit_leaves_host_state_and_effects_unchanged() {
        use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};
        use jefe::domain::plugin::field::Scalar;

        let (mut state, panel) = active_form(Some(Scalar::Integer(3)));
        let prior = state.provider_panels.host_local(panel).cloned();
        let key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x'));

        assert!(edit_form_field(&state, panel, &key).is_none());
        assert_eq!(state.provider_panels.host_local(panel), prior.as_ref());
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn stale_raw_form_edit_leaves_host_state_and_effects_unchanged() {
        use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};

        let (mut state, panel) = active_form(None);
        state
            .provider_panels
            .fail_runtime(panel)
            .unwrap_or_else(|error| panic!("runtime failure: {error}"));
        let prior = state.provider_panels.host_local(panel).cloned();
        let key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x'));

        assert!(edit_form_field(&state, panel, &key).is_none());
        assert_eq!(state.provider_panels.host_local(panel), prior.as_ref());
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn protected_raw_keys_are_not_interpreted_as_form_edits() {
        use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

        let (mut state, panel) = active_form(None);
        let escape = KeyEvent::new(KeyEventKind::Press, KeyCode::Esc);
        let mut emergency = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('q'));
        emergency.modifiers = KeyModifiers::CONTROL;

        assert!(edit_form_field(&state, panel, &escape).is_none());
        assert!(edit_form_field(&state, panel, &emergency).is_none());
        assert!(state.provider_panels.host_local(panel).is_none());
        assert!(state.take_staged_effects().is_empty());
    }
}
