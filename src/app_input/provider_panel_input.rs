use jefe::runtime::provider::protocol::{PanelBody, PanelEvent};
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
        let Some(event) = event_for_action(action, &state, panel) else {
            return;
        };
        state.submit_provider_panel_event(panel, event);
        state.take_staged_effects()
    };
    super::provider_dispatch::schedule_provider_effects(app_state, ctx, staged);
}

fn event_for_action(
    action: BoundaryAction,
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<PanelEvent> {
    match action {
        BoundaryAction::ProviderPanelPrevious => select_list_item(state, panel, false),
        BoundaryAction::ProviderPanelNext => select_list_item(state, panel, true),
        BoundaryAction::ProviderPanelActivate => {
            selected_item(state, panel).map(|id| PanelEvent::Activated { id })
        }
        BoundaryAction::ProviderPanelRetry => Some(PanelEvent::Retry),
        BoundaryAction::ProviderPanelCancel => Some(PanelEvent::Cancel),
        _ => None,
    }
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
    let selected = state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.selected_id.as_ref())
        .or(body.selected_id.as_ref());
    let current = selected.and_then(|id| body.items.iter().position(|item| &item.id == id));
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

fn selected_item(
    state: &jefe::state::AppState,
    panel: PanelInstanceId,
) -> Option<jefe::domain::Id> {
    state
        .provider_panels
        .host_local(panel)
        .and_then(|local| local.selected_id.clone())
        .or_else(|| {
            let snapshot = state.provider_panels.accepted_snapshot(panel)?;
            let PanelBody::List(body) = &snapshot.body else {
                return None;
            };
            body.selected_id.clone()
        })
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
}
