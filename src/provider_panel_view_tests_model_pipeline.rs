fn assert_provider_form_projection(
    panel: &PanelProjection,
    expected_status: PanelStatus,
    stale: bool,
) {
    let text = panel.lines.join("\n");
    assert_eq!(panel.status, expected_status);
    for expected in ["loading…", "Provider description", "host draft"] {
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
    }
    assert_eq!(text.contains("stale"), stale, "unexpected staleness: {text:?}");
    assert!(!text.contains("snapshot"), "host draft must win: {text:?}");
}

#[test]
fn provider_model_projection_preserves_shared_metadata_staleness_form_local_and_affordances() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Form],
    );
    let query = id("query");
    let mut values = TypedMap::new();
    values.insert(query.clone(), TypedValue::String("snapshot".to_owned()));
    let body = PanelBody::Form(FormBody {
        fields: vec![string_field("query", "Query")],
        values,
        field_errors: Vec::new(),
        submit_action: action_id("vendor.run"),
    });
    let mut snapshot = snapshot_with_body(panel, 1, body, BodyKind::Form);
    snapshot.description = Some("Provider description".to_owned());
    snapshot.loading = true;
    snapshot.action_affordances = vec![affordance("submit", "Submit", true, None)];
    accept_snapshot(&mut state, panel, snapshot);
    let mut draft = TypedMap::new();
    draft.insert(query, TypedValue::String("host draft".to_owned()));
    state
        .update_host_local(
            panel,
            HostLocal {
                focus_target: None,
                scroll_offset: 0,
                selected_id: None,
                form_draft: Some(draft),
            },
        )
        .unwrap_or_else(|error| panic!("form-local fixture: {error}"));
    let active = project_view(&descriptor, &state, &layout);
    let active_panel = projected_panel(&active, "main");
    assert_provider_form_projection(active_panel, PanelStatus::Active, false);
    assert!(
        active_panel
            .hit_targets
            .contains(&Some(PanelHitTarget::Submit))
    );

    state
        .fail_runtime(panel)
        .unwrap_or_else(|error| panic!("stale fixture: {error}"));
    let failed_view = project_view(&descriptor, &state, &layout);
    assert_provider_form_projection(
        projected_panel(&failed_view, "main"),
        PanelStatus::Failed,
        true,
    );
}
