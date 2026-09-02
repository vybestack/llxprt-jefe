use super::action_handlers::BoundaryAction;
use jefe::domain::{Id, TypedValue};
use jefe::host_controls::{ControlAction, ControlIntent, control_intent, selected_control_id};
use jefe::provider_panel_view::PanelHitTarget;
use jefe::runtime::provider::protocol::{PanelBody, PanelEvent};
use jefe::state::provider_panels::PanelInstanceId;

pub(super) fn apply(
    action: BoundaryAction,
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
) {
    let staged = {
        let mut state = app_state.write();
        let current = state.nav.current().clone();
        let Some(projection) = panel_projection(&state, &current.panel_focus) else {
            return;
        };
        let panel = state
            .provider_panels()
            .panel_for_screen(current.id.get(), &current.panel_focus);
        let provider_bound = state
            .published_workbench()
            .screen_registry()
            .panel_binding(current.screen, &current.panel_focus)
            .is_some();
        let Some(panel) = panel else {
            if provider_bound {
                return;
            }
            let Some(capability) = current_host_panel_capability(&state, &current.panel_focus)
            else {
                return;
            };
            let Some(control_action) = boundary_control_action(action) else {
                return;
            };
            let (viewport_cols, viewport_rows) =
                jefe::screen_layout::committed_render_size_or_content(
                    state.resolved_layout.as_ref(),
                    &projection.content,
                );
            let _ = state.apply_host_panel_action(
                capability,
                control_action,
                usize::from(viewport_cols),
                usize::from(viewport_rows),
            );
            return;
        };
        if state.provider_panels().accepted_model_is_stale(panel) {
            return;
        }
        let max_scroll_offset = projection.max_scroll_offset;
        if let Some(event) = event_for_action(action, &state, panel) {
            if !state.submit_provider_panel_semantic_event(panel, &current.panel_focus, event) {
                return;
            }
        } else if !apply_local_action(action, &mut state, panel, max_scroll_offset) {
            return;
        }
        state.take_staged_effects()
    };
    super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
}

/// Apply one workbench paging key through the shared control path.
///
/// The card grid owns split-screen paging regardless of which panel holds
/// focus — the behavior the legacy `split.page-up`/`split.page-down` events
/// carried — so this resolves the screen's declared cards control rather
/// than the focused panel. The control factory bounds the step by the
/// committed frame's display basis and keeps it inert without one
/// (issue #706).
pub(super) fn apply_workbench_paging(
    action: ControlAction,
    app_state: &mut super::AppStateHandle,
    ctx: &super::SharedContext,
) {
    let staged = {
        let mut state = app_state.write();
        let Some(capability) = workbench_cards_capability(&state) else {
            return;
        };
        // Paging reads the display basis, not a panel content rectangle, and
        // without a committed frame the reducer is inert, so no fallback
        // geometry is invented here.
        let (viewport_cols, viewport_rows) =
            state.resolved_layout.as_ref().map_or((0, 0), |layout| {
                let (cols, rows) = jefe::screen_layout::committed_render_size(layout);
                (usize::from(cols), usize::from(rows))
            });
        let _ = state.apply_host_panel_action(capability, action, viewport_cols, viewport_rows);
        state.take_staged_effects()
    };
    super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
}

/// The active screen's declared card-grid control, if it declares one.
fn workbench_cards_capability(
    state: &jefe::state::AppState,
) -> Option<jefe::workbench::HostPanelCapability> {
    state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?
        .panels
        .iter()
        .find(|panel| {
            panel.host_capability().is_some_and(|capability| {
                capability.model_source() == jefe::workbench::HostPanelModelSource::WorkbenchCards
            })
        })
        .and_then(jefe::workbench::PanelDescriptor::host_capability)
}

fn current_host_panel_capability(
    state: &jefe::state::AppState,
    panel_id: &jefe::workbench::PanelId,
) -> Option<jefe::workbench::HostPanelCapability> {
    state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?
        .panels
        .iter()
        .find(|panel| panel.id == *panel_id)?
        .host_capability()
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
        .provider_panels()
        .panel_for_screen(current.id.get(), &projection.id);
    if panel.is_some_and(|instance| state.provider_panels().accepted_model_is_stale(instance)) {
        return (false, None);
    }
    let host_owned = panel.is_none()
        && state
            .published_workbench()
            .screen_registry()
            .panel_binding(current.screen, &projection.id)
            .is_none();
    match action {
        ProviderPanelMouseAction::ScrollUp | ProviderPanelMouseAction::ScrollDown => {
            let consumed = if let Some(instance) = panel {
                scroll_mouse_panel(state, instance, action, projection.max_scroll_offset)
            } else if host_owned
                && let Some(capability) = current_host_panel_capability(state, &projection.id)
            {
                let delta = if action == ProviderPanelMouseAction::ScrollUp {
                    -1
                } else {
                    1
                };
                state.scroll_host_panel(capability, delta, usize::from(projection.content.height))
            } else {
                false
            };
            (consumed, None)
        }
        ProviderPanelMouseAction::Click => {
            let target = hit_target(&projection, col, row);
            if host_owned
                && let Some(capability) = current_host_panel_capability(state, &projection.id)
            {
                let consumed = apply_host_owned_click(state, capability, &projection, target);
                return (consumed, None);
            }
            apply_mouse_target(state, panel, projection.id, target)
        }
    }
}

/// Apply a click on a host-owned panel through the shared target→action
/// semantics, sizing the control from the committed render viewport.
fn apply_host_owned_click(
    state: &mut jefe::state::AppState,
    capability: jefe::workbench::HostPanelCapability,
    projection: &jefe::provider_panel_view::PanelProjection,
    target: Option<jefe::provider_panel_view::PanelHitTarget>,
) -> bool {
    // Host-owned panels share the provider target→action
    // semantics: an unselected item selects, a selected item
    // activates; Submit/Action/paging/retry/cancel target the same
    // affordances. Failures return unconsumed so the terminal never
    // treats a host click as its own.
    let action = target.and_then(shared_host_target_action);
    action.is_some_and(|action| {
        let (viewport_cols, viewport_rows) = jefe::screen_layout::committed_render_size_or_content(
            state.resolved_layout.as_ref(),
            &projection.content,
        );
        state.apply_host_panel_action(
            capability,
            action,
            usize::from(viewport_cols),
            usize::from(viewport_rows),
        )
    })
}

fn apply_mouse_target(
    state: &mut jefe::state::AppState,
    panel: Option<PanelInstanceId>,
    panel_id: jefe::workbench::PanelId,
    target: Option<jefe::provider_panel_view::PanelHitTarget>,
) -> (bool, Option<Vec<jefe::domain::effects::IssuedEffect>>) {
    let Some(instance) = panel else {
        return (false, None);
    };
    if matches!(target, Some(PanelHitTarget::Unavailable)) {
        return (false, None);
    }
    if let Some(PanelHitTarget::Field(field_id)) = target {
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
    let Some(event) = mouse_event(state, instance, target) else {
        return (false, None);
    };
    if !state.submit_provider_panel_semantic_event(instance, &panel_id, event) {
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
    let layout = state
        .resolved_layout
        .as_ref()
        .filter(|layout| layout.screen_instance == state.nav.current().id)?;
    let registry = state.published_workbench().screen_registry();
    let descriptor = registry.get_identity(state.screen())?;
    jefe::provider_panel_view::project_current_screen(state, descriptor, layout)
        .ok()?
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
    // Rectangle-keyed targets (the card grid) win before the row-indexed
    // fallback: a grid packs several targets onto one row (issue #706).
    if let Some(target) = projection
        .rect_hit_targets
        .iter()
        .find(|(rect, _)| rect.contains(col, row))
        .map(|(_, target)| target.clone())
    {
        return Some(target);
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
        .provider_panels()
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
        .provider_panels_mut()
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
        .provider_panels()
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    state
        .provider_panels_mut()
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
    match &target {
        // A selected item click activates it; an unselected one selects it. This
        // shares the provider surface's target semantics for host-owned controls so
        // identical visible rows behave identically.
        jefe::provider_panel_view::PanelHitTarget::ListItem(id)
        | jefe::provider_panel_view::PanelHitTarget::TreeNode(id)
        | jefe::provider_panel_view::PanelHitTarget::DiffFile(id) => {
            let action = if selected_item(state, panel).as_ref() == Some(id) {
                ControlAction::Activate
            } else {
                ControlAction::Select(id.clone())
            };
            control_event(state, panel, action)
        }
        jefe::provider_panel_view::PanelHitTarget::Action(id) => {
            control_event(state, panel, ControlAction::Action(id.clone()))
        }
        jefe::provider_panel_view::PanelHitTarget::Submit => {
            control_event(state, panel, ControlAction::Submit)
        }
        jefe::provider_panel_view::PanelHitTarget::PageRequested => {
            control_event(state, panel, ControlAction::PageNext)
        }
        jefe::provider_panel_view::PanelHitTarget::Retry => {
            control_event(state, panel, ControlAction::Retry)
        }
        jefe::provider_panel_view::PanelHitTarget::Cancel => {
            control_event(state, panel, ControlAction::Cancel)
        }
        jefe::provider_panel_view::PanelHitTarget::Link(id) => {
            control_event(state, panel, ControlAction::Link(id.clone()))
        }
        jefe::provider_panel_view::PanelHitTarget::Field(_)
        | jefe::provider_panel_view::PanelHitTarget::Unavailable => None,
    }
}

/// Map one provider/host mouse hit target to the shared control action. This is
/// the same pure mapping the provider surface uses so identical visible rows behave
/// identically whether they are provider-owned or host-controlled.
fn shared_host_target_action(
    target: jefe::provider_panel_view::PanelHitTarget,
) -> Option<ControlAction> {
    match target {
        jefe::provider_panel_view::PanelHitTarget::ListItem(id)
        | jefe::provider_panel_view::PanelHitTarget::TreeNode(id)
        | jefe::provider_panel_view::PanelHitTarget::DiffFile(id) => {
            Some(ControlAction::Select(id))
        }
        jefe::provider_panel_view::PanelHitTarget::Action(id) => Some(ControlAction::Action(id)),
        jefe::provider_panel_view::PanelHitTarget::Submit => Some(ControlAction::Submit),
        jefe::provider_panel_view::PanelHitTarget::PageRequested => Some(ControlAction::PageNext),
        jefe::provider_panel_view::PanelHitTarget::Retry => Some(ControlAction::Retry),
        jefe::provider_panel_view::PanelHitTarget::Cancel => Some(ControlAction::Cancel),
        jefe::provider_panel_view::PanelHitTarget::Link(id) => Some(ControlAction::Link(id)),
        jefe::provider_panel_view::PanelHitTarget::Field(_)
        | jefe::provider_panel_view::PanelHitTarget::Unavailable => None,
    }
}

fn boundary_control_action(action: BoundaryAction) -> Option<ControlAction> {
    match action {
        BoundaryAction::ProviderPanelPrevious => Some(ControlAction::Previous),
        BoundaryAction::ProviderPanelNext => Some(ControlAction::Next),
        BoundaryAction::ProviderPanelActivate => Some(ControlAction::Activate),
        BoundaryAction::ProviderPanelRetry => Some(ControlAction::Retry),
        BoundaryAction::ProviderPanelCancel => Some(ControlAction::Cancel),
        BoundaryAction::ProviderPanelAction => Some(ControlAction::FocusedAction),
        BoundaryAction::ProviderPanelSubmit => Some(ControlAction::Submit),
        BoundaryAction::ProviderPanelPageNext => Some(ControlAction::PageNext),
        BoundaryAction::ProviderPanelLinkSelect => Some(ControlAction::FocusedLink),
        _ => None,
    }
}

fn event_for_action(
    action: BoundaryAction,
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<PanelEvent> {
    control_event(state, panel, boundary_control_action(action)?)
}

fn apply_local_action(
    action: BoundaryAction,
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    max_scroll_offset: u32,
) -> bool {
    let control_action = match action {
        BoundaryAction::ProviderPanelPrevious => ControlAction::Previous,
        BoundaryAction::ProviderPanelNext => ControlAction::Next,
        _ => return false,
    };
    let ControlIntent::Scroll(direction) = control_intent_for_state(state, panel, control_action)
    else {
        return false;
    };
    let prior = state
        .provider_panels()
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
    state
        .provider_panels_mut()
        .update_host_local(panel, host)
        .is_ok()
}

fn control_intent_for_state(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    action: ControlAction,
) -> ControlIntent {
    let Some(snapshot) = state.provider_panels().accepted_snapshot(panel) else {
        return ControlIntent::None;
    };
    let local = state.provider_panels().host_local(panel);
    control_intent(
        snapshot,
        local.and_then(|value| value.selected_id.as_ref()),
        local.and_then(|value| value.focus_target.as_ref()),
        local.and_then(|value| value.form_draft.as_ref()),
        action,
    )
}

fn control_event(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    action: ControlAction,
) -> Option<PanelEvent> {
    match control_intent_for_state(state, panel, action) {
        ControlIntent::Event(event) => Some(event),
        ControlIntent::Scroll(_) | ControlIntent::PagePrevious | ControlIntent::None => None,
    }
}

fn selected_item(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<jefe::domain::Id> {
    let snapshot = state.provider_panels().accepted_snapshot(panel)?;
    let local = state
        .provider_panels()
        .host_local(panel)
        .and_then(|value| value.selected_id.as_ref());
    selected_control_id(snapshot, local).cloned()
}

enum RawKeyMutation {
    Local,
    Draft { field_id: Id, value: TypedValue },
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
        let workbench = std::sync::Arc::clone(state.published_workbench());
        let registry = workbench.screen_registry();
        if registry
            .panel_binding(current.screen, &current.panel_focus)
            .is_none()
        {
            return false;
        }
        let Some(panel) = state
            .provider_panels()
            .panel_for_screen(current.id.get(), &current.panel_focus)
        else {
            return false;
        };
        if state.provider_panels().accepted_model_is_stale(panel) {
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
            Some(RawKeyMutation::Draft { field_id, value }) => (
                update_form_draft(&mut state, panel, field_id, value),
                Vec::new(),
            ),
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
    let Some(snapshot) = state.provider_panels().accepted_snapshot(panel) else {
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
        .provider_panels()
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
    state
        .provider_panels_mut()
        .update_host_local(panel, host)
        .is_ok()
}

fn update_form_draft(
    state: &mut jefe::state::AppState,
    panel: PanelInstanceId,
    field_id: Id,
    value: TypedValue,
) -> bool {
    let mut host = state
        .provider_panels()
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    host.form_draft
        .get_or_insert_default()
        .insert(field_id, value);
    state
        .provider_panels_mut()
        .update_host_local(panel, host)
        .is_ok()
}

fn edit_form_field(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
    key_event: &iocraft::prelude::KeyEvent,
) -> Option<RawKeyMutation> {
    use iocraft::prelude::{KeyCode, KeyModifiers};
    use jefe::domain::plugin::FieldKind;
    use jefe::form_value_edit::{FormValueEdit, edit_form_value, form_value_is_complete};

    if key_event.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META,
    ) {
        return None;
    }
    if state.provider_panels().accepted_model_is_stale(panel) {
        return None;
    }
    let snapshot = state.provider_panels().accepted_snapshot(panel)?;
    let PanelBody::Form(form) = &snapshot.body else {
        return None;
    };
    let prior = state
        .provider_panels()
        .host_local(panel)
        .cloned()
        .unwrap_or_default();
    let field = prior
        .focus_target
        .as_ref()
        .and_then(|id| form.fields.iter().find(|field| field.id() == id))?
        .clone();
    let current = prior
        .form_draft
        .as_ref()
        .and_then(|draft| draft.get(field.id()))
        .or_else(|| form.values.get(field.id()));
    let edit = match (field.kind(), key_event.code) {
        (FieldKind::Boolean, KeyCode::Char(' ')) => FormValueEdit::Toggle,
        (FieldKind::Enum, KeyCode::Char(' ')) => FormValueEdit::NextChoice,
        (_, KeyCode::Char(character)) => FormValueEdit::Character(character),
        (_, KeyCode::Backspace) => FormValueEdit::Backspace,
        _ => return None,
    };
    let value = edit_form_value(&field, current, edit)?;
    if form_value_is_complete(&field, &value) {
        Some(RawKeyMutation::Event(field_change_event(
            field.id().clone(),
            value,
        )))
    } else {
        Some(RawKeyMutation::Draft {
            field_id: field.id().clone(),
            value,
        })
    }
}

/// Construct a PanelEvent::FieldChanged for a form field edit.
#[must_use]
fn field_change_event(field_id: Id, value: TypedValue) -> PanelEvent {
    PanelEvent::FieldChanged { field_id, value }
}

#[cfg(test)]
mod tests {
    include!("provider_panel_input_tests_core.rs");
    include!("provider_panel_input_tests_interaction.rs");
    include!("provider_panel_input_tests_live.rs");
}
