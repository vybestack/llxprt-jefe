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
