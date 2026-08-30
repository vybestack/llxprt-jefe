    fn compiled_host_capability(
        state: &jefe::state::AppState,
        panel_type: &str,
    ) -> jefe::workbench::HostPanelCapability {
        state
            .published_workbench()
            .screen_registry()
            .get_identity(state.screen())
            .and_then(|descriptor| {
                descriptor
                    .panels
                    .iter()
                    .find(|panel| panel.panel_type.as_str() == panel_type)
            })
            .and_then(jefe::workbench::PanelDescriptor::host_capability)
            .unwrap_or_else(|| panic!("compiled {panel_type} capability"))
    }

    #[test]
    fn unbound_host_list_uses_shared_control_intents_and_instance_viewport() {
        let mut state = crate::test_app_state();
        state.hide_idle_repositories = false;
        for (id, name) in [("one", "One"), ("two", "Two"), ("three", "Three")] {
            state.repositories.push(jefe::domain::Repository::new(
                jefe::domain::RepositoryId(id.to_owned()),
                jefe::domain::agent_definition::AgentTypeId::default(),
                jefe::domain::TypedMap::new(),
                name.to_owned(),
                id.to_owned(),
                std::path::PathBuf::from(format!("/{id}")),
            ));
        }
        let repository_capability = compiled_host_capability(&state, "repository-list");

        assert!(state.apply_host_panel_action(
            repository_capability,
            ControlAction::Next,
            1,
        ));
        assert_eq!(
            state.selected_repository().map(|repository| repository.id.0.as_str()),
            Some("two")
        );
        assert_eq!(state.repository_scroll_offset, 1);

        assert!(state.apply_host_panel_action(
            repository_capability,
            ControlAction::Next,
            1,
        ));
        assert_eq!(
            state.selected_repository().map(|repository| repository.id.0.as_str()),
            Some("three")
        );
        assert_eq!(state.repository_scroll_offset, 2);

        assert!(!state.scroll_host_panel(repository_capability, 1, 1));
        assert!(state.scroll_host_panel(repository_capability, -1, 1));
        assert_eq!(state.repository_scroll_offset, 1);
    }

    #[test]
    fn unbound_host_form_uses_shared_submit_intent_on_the_exact_instance() {
        let mut state = crate::test_app_state();
        let owner = state.nav.current().id;
        let search_capability = compiled_host_capability(&state, "search-input");

        assert!(state.apply_host_panel_action(
            search_capability,
            ControlAction::Submit,
            4,
        ));

        assert_eq!(state.nav.current().id, owner);
        assert_eq!(state.active_overlay_kind(), Some(jefe::workbench::OverlayKind::Search));
    }

    #[test]
    fn keyboard_projection_requires_the_frame_owned_layout() {
        let mut state = crate::test_app_state();
        let panel = state.nav.current().panel_focus;
        assert!(state.resolved_layout.is_none());
        assert!(panel_projection(&state, &panel).is_none());

        state.resolved_layout = jefe::screen_layout::resolve_screen(&state, 120, 40);
        assert!(panel_projection(&state, &panel).is_some());
    }
