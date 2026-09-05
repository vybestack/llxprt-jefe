    use jefe::domain::{Id, TypedMap, TypedValue};
    use jefe::runtime::provider::protocol::{
        Affordance, BodyKind, HostLocal, ListBody, ListItem, PanelSnapshot, StructuredDiffBody,
        StructuredDiffFile, StructuredDiffPath, TreeBody, TreeNode,
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
                    plain_item(alpha.clone(), "Alpha"),
                    plain_item(beta, "Beta"),
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
        let mut state = crate::test_app_state();
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
            .provider_panels_mut()
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
            .provider_panels_mut()
            .activate(declared.instance)
            .unwrap_or_else(|error| panic!("activate: {error}"));
        let alpha = Id::parse("alpha").unwrap_or_else(|error| panic!("alpha: {error}"));
        let beta = Id::parse("beta").unwrap_or_else(|error| panic!("beta: {error}"));
        let snapshot = list_snapshot(declared.instance, alpha, beta);
        state
            .provider_panels_mut()
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
    fn active_selectable_panel(body: PanelBody, kind: BodyKind) -> (AppState, PanelInstanceId) {
        let owner = id("vendor.panel");
        let panel_type = id("vendor.panel.selectable");
        let panel_id = PanelId::from_static("main");
        let mut state = crate::test_app_state();
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
                kind: EventKind::ExpansionChanged,
                arguments: Vec::new(),
            },
        ];
        let declared = state
            .provider_panels_mut()
            .declare(DeclareInput {
                owner: &owner,
                panel_id: &panel_id,
                screen_instance_id: 7,
                panel_type: &panel_type,
                activation: &TypedMap::new(),
                allowed_model_kinds: &[kind],
                allowed_events: &allowed_events,
                action_authority: &[],
                process_generation: 1,
            })
            .unwrap_or_else(|error| panic!("declare: {error}"));
        state
            .provider_panels_mut()
            .activate(declared.instance)
            .unwrap_or_else(|error| panic!("activate: {error}"));
        let snapshot = snapshot_with_body(declared.instance, body, kind, Vec::new());
        state
            .provider_panels_mut()
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

    fn accept_next_snapshot(
        state: &mut AppState,
        panel: PanelInstanceId,
        body: PanelBody,
        kind: BodyKind,
        affordances: Vec<Affordance>,
    ) {
        let owner = id("vendor.panel");
        let mut snapshot = snapshot_with_body(panel, body, kind, affordances);
        snapshot.revision = 2;
        state
            .provider_panels_mut()
            .accept_snapshot(AcceptSnapshot {
                owner: &owner,
                received_process_generation: 1,
                payload_byte_count: 256,
                elapsed_ms: 0,
                snapshot: &snapshot,
            })
            .unwrap_or_else(|error| panic!("next snapshot: {error}"));
    }


    fn select_and_commit(
        state: &mut AppState,
        panel: PanelInstanceId,
        forward: bool,
    ) -> Option<PanelEvent> {
        let event = control_event(
            state,
            panel,
            if forward {
                ControlAction::Next
            } else {
                ControlAction::Previous
            },
        )?;
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
                .provider_panels()
                .host_local(panel)
                .and_then(|local| local.selected_id.as_ref())
                .map(Id::as_str),
            Some("beta")
        );
    }

    #[test]
    fn rejected_selected_event_does_not_mutate_host_selection() {
        let (mut state, panel) = active_list();
        let event = control_event(&state, panel, ControlAction::Next)
            .unwrap_or_else(|| panic!("list selection must produce an event"));
        state
            .provider_panels_mut()
            .suspend(panel)
            .unwrap_or_else(|error| panic!("suspend: {error}"));

        assert!(!state.submit_provider_panel_event(panel, event));
        assert_eq!(
            state
                .provider_panels()
                .host_local(panel)
                .and_then(|local| local.selected_id.as_ref()),
            None
        );
    }

    #[test]
    fn activate_uses_optimistic_host_selection_while_provider_response_is_pending() {
        let (mut state, panel) = active_list();
        let _ = select_and_commit(&mut state, panel, true);

        assert!(matches!(
            selected_item(&state, panel),
            Some(id) if id.as_str() == "beta"
        ));
    }

    /// A list item with nothing but an identity and a label: no description,
    /// no status word, no count.
    ///
    /// The four construction sites share this rather than spelling the literal
    /// out, because `ListItem` gaining a field (#745) put the expanded form of
    /// this file two lines over the 1000-line hard limit `xtask check
    /// source-size` enforces.
    fn plain_item(id: Id, label: &str) -> ListItem {
        ListItem {
            id,
            label: label.to_owned(),
            description: None,
            status: None,
            count: None,
            actions: Vec::new(),
        }
    }

    fn authoritative_list(first: &Id, second: &Id) -> PanelBody {
        PanelBody::List(ListBody {
            items: vec![
                plain_item(first.clone(), "First"),
                plain_item(second.clone(), "Second"),
            ],
            selected_id: Some(first.clone()),
            next_page_token: None,
        })
    }

    fn authoritative_tree(first: &Id, second: &Id) -> PanelBody {
        PanelBody::Tree(TreeBody {
            schema_version: 1,
            nodes: vec![
                TreeNode {
                    id: first.clone(),
                    parent_id: None,
                    label: "First".to_owned(),
                    semantic_key: id("first-key"),
                    depth: 0,
                    expandable: true,
                    expanded: true,
                },
                TreeNode {
                    id: second.clone(),
                    parent_id: Some(first.clone()),
                    label: "Second".to_owned(),
                    semantic_key: id("second-key"),
                    depth: 1,
                    expandable: false,
                    expanded: false,
                },
            ],
            selected_id: Some(first.clone()),
        })
    }

    fn authoritative_diff(first: &Id, second: &Id) -> PanelBody {
        let file = |id: Id, path: &str| StructuredDiffFile {
            id,
                path: StructuredDiffPath::Added(path.to_owned()),

            old_mode: None,
            new_mode: None,
            binary: true,
            hunks: Vec::new(),
        };
        PanelBody::StructuredDiff(StructuredDiffBody {
            schema_version: 1,
            files: vec![file(first.clone(), "first"), file(second.clone(), "second")],
            selected_file_id: Some(first.clone()),
        })
    }

    #[test]
    fn accepted_snapshots_restore_authoritative_list_tree_and_diff_selection() {
        let first = id("first");
        let second = id("second");
        let fixtures = [
            (authoritative_list(&first, &second), BodyKind::List),
            (authoritative_tree(&first, &second), BodyKind::Tree),
            (authoritative_diff(&first, &second), BodyKind::StructuredDiff),
        ];
        for (body, kind) in fixtures {
            let (mut state, panel) = active_selectable_panel(body.clone(), kind);
            assert!(matches!(
                select_and_commit(&mut state, panel, true),
                Some(PanelEvent::Selected { ref id }) if id == &second
            ));
            accept_next_snapshot(&mut state, panel, body, kind, Vec::new());
            let targeted = match control_event(&state, panel, ControlAction::Activate) {
                Some(PanelEvent::Activated { id } | PanelEvent::ExpansionChanged { id, .. }) => id,
                event => panic!("provider-selected activation event: {event:?}"),
            };
            assert_eq!(targeted, first);
        }
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
                .provider_panels_mut()
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
            .provider_panels_mut()
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
                .provider_panels_mut()
                .accept_snapshot(AcceptSnapshot {
                    owner: &owner,
                    received_process_generation: 1,
                    payload_byte_count: 524_289,
                    elapsed_ms: 1,
                    snapshot: &invalid,
                })
                .is_err()
        );
        assert!(state.provider_panels().accepted_snapshot(panel).is_none());

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
            if state.provider_panels_mut().update_host_local(panel, host).is_ok() {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "a largest valid host-local fixture must be found");
        let prior = state
            .provider_panels()
            .host_local(panel)
            .cloned()
            .unwrap_or_else(|| panic!("host-local fixture must be retained"));
        let selected = PanelEvent::Selected {
            id: Id::parse("beta").unwrap_or_else(|error| panic!("beta: {error}")),
        };

        assert!(!state.submit_provider_panel_event(panel, selected));
        assert_eq!(state.provider_panels().host_local(panel), Some(&prior));
        assert!(state.take_staged_effects().is_empty());
    }

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
        let mut state = crate::test_app_state();
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
                .provider_panels()
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
                .provider_panels()
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
        let prior_local = state.provider_panels().host_local(panel).cloned();
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
            state.provider_panels().host_local(panel),
            prior_local.as_ref()
        );
        assert_eq!(state.error_message.as_deref(), Some("existing error"));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn mouse_target_without_provider_instance_is_not_consumed() {
        let mut state = crate::test_app_state();
        let prior_focus = state.nav.current().panel_focus;

        let (consumed, staged) = apply_mouse_target(
            &mut state,
            None,
            PanelId::from_static("pty-terminal"),
            None,
        );

        assert!(!consumed);
        assert!(staged.is_none());
        assert_eq!(state.nav.current().panel_focus, prior_focus);
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn stale_mouse_target_preserves_focus_error_local_state_and_effects() {
        let (mut state, panel) = active_list();
        let prior_focus = state.nav.current().panel_focus;
        let prior_local = state.provider_panels().host_local(panel).cloned();
        state.error_message = Some("existing error".to_owned());
        state
            .provider_panels_mut()
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
            state.provider_panels().host_local(panel),
            prior_local.as_ref()
        );
        assert_eq!(state.error_message.as_deref(), Some("existing error"));
        assert!(state.take_staged_effects().is_empty());
    }

    #[test]
    fn tree_mouse_activation_emits_expansion_without_mutating_provider_truth() {
        let root = id("root");
        let body = PanelBody::Tree(TreeBody {
            schema_version: 1,
            nodes: vec![TreeNode {
                id: root.clone(),
                parent_id: None,
                label: "Root".to_owned(),
                semantic_key: id("root-key"),
                depth: 0,
                expandable: true,
                expanded: false,
            }],
            selected_id: Some(root.clone()),
        });
        let (mut state, panel) = active_selectable_panel(body, BodyKind::Tree);

        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::TreeNode(root.clone())),
            Some(PanelEvent::ExpansionChanged {
                id: root.clone(),
                expanded: true,
            })
        );
        let (consumed, staged) = apply_mouse_target(
            &mut state,
            Some(panel),
            PanelId::from_static("main"),
            Some(PanelHitTarget::TreeNode(root.clone())),
        );

        assert!(consumed);
        assert_eq!(staged.as_ref().map(Vec::len), Some(1));
        let snapshot = state
            .provider_panels()
            .accepted_snapshot(panel)
            .unwrap_or_else(|| panic!("tree snapshot"));
        let PanelBody::Tree(tree) = &snapshot.body else {
            panic!("tree body")
        };
        assert!(!tree.nodes[0].expanded);
    }

    #[test]
    fn structured_diff_mouse_selects_then_activates_files() {
        let first = id("first");
        let second = id("second");
        let file = |id: Id, path: &str| StructuredDiffFile {
            id,
                path: StructuredDiffPath::Added(path.to_owned()),

            old_mode: None,
            new_mode: None,
            binary: true,
            hunks: Vec::new(),
        };
        let body = PanelBody::StructuredDiff(StructuredDiffBody {
            schema_version: 1,
            files: vec![file(first.clone(), "first"), file(second.clone(), "second")],
            selected_file_id: Some(first.clone()),
        });
        let (state, panel) = active_selectable_panel(body, BodyKind::StructuredDiff);

        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::DiffFile(second.clone())),
            Some(PanelEvent::Selected { id: second })
        );
        assert_eq!(
            mouse_event(&state, panel, PanelHitTarget::DiffFile(first.clone())),
            Some(PanelEvent::Activated { id: first })
        );
    }

    #[test]
    fn mouse_targets_invalid_for_the_active_control_are_rejected() {
        let (state, panel) = active_list();

        for target in [
            PanelHitTarget::Action(id("undeclared")),
            PanelHitTarget::Retry,
            PanelHitTarget::Cancel,
            PanelHitTarget::Link(id("details")),
            PanelHitTarget::Unavailable,
        ] {
            assert!(
                mouse_event(&state, panel, target).is_none(),
                "the active list factory must reject another control's target"
            );
        }
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
                .provider_panels()
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
                .provider_panels()
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

        let mut retry_state = crate::test_app_state();
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

        let mut cancel_state = crate::test_app_state();
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
            .provider_panels_mut()
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
            .provider_panels_mut()
            .activate(declared.instance)
            .unwrap_or_else(|error| panic!("activate: {error}"));
        let mut snapshot = snapshot.clone();
        snapshot.panel_instance_id = declared.instance.as_u64();
        state
            .provider_panels_mut()
            .accept_snapshot(AcceptSnapshot {
                owner: identity.0,
                received_process_generation: 1,
                payload_byte_count: 256,
                elapsed_ms: 0,
                snapshot: &snapshot,
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
                kind: EventKind::ExpansionChanged,
                arguments: Vec::new(),
            },
            EventDeclaration {
                kind: EventKind::LinkSelected,
                arguments: Vec::new(),
            },
        ]
    }

    // ── Action event tests ───────────────────────────────────────────────
