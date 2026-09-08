#[test]
fn host_list_projection_carries_spans_aligned_with_authoritative_text() {
    let mut panel = super::unavailable_panel(
        PanelId::from_static("main"),
        true,
        Rect::new(0, 0, 42, 5),
        Rect::new(1, 1, 40, 3),
    );
    let model = crate::host_panel_models::HostPanelModel {
        title: "Fixture".to_owned(),
        body: PanelBody::List(ListBody {
            items: vec![ListItem {
                id: id("alpha"),
                label: "Alpha".to_owned(),
                description: None,
                status: None,
                count: None,
                glyph: Some(crate::runtime::provider::protocol::ListItemGlyph {
                    text: "~".to_owned(),
                    role: crate::runtime::provider::protocol::ListItemGlyphRole::Yellow,
                }),
                badge: Some("1".to_owned()),
                suffix: Some("  owner/repo @ main".to_owned()),
                actions: Vec::new(),
            }],
            selected_id: None,
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id: None,
        grabbed_id: None,
        scroll_offset: 0,
        reveal_selection: true,
    };

    super::project_host_model(&mut panel, model);

    assert_eq!(panel.lines, vec![">> ~ [1] Alpha  owner/repo @ main"]);
    assert_eq!(panel.spans.len(), panel.lines.len());
    assert_eq!(
        panel.spans[0]
            .iter()
            .map(|span| (span.text.as_str(), span.role))
            .collect::<Vec<_>>(),
        vec![
            (">> ", crate::host_controls::HostControlSpanRole::Themed),
            ("~", crate::host_controls::HostControlSpanRole::Yellow),
            (" [1] Alpha", crate::host_controls::HostControlSpanRole::Themed),
            (
                "  owner/repo @ main",
                crate::host_controls::HostControlSpanRole::Dim,
            ),
        ]
    );
}

#[test]
fn host_list_projection_reveals_selected_row_outside_retained_window() {
    let content = Rect::new(1, 1, 20, 3);
    let mut panel = super::unavailable_panel(
        PanelId::from_static("repositories"),
        true,
        Rect::new(0, 0, 22, 5),
        content,
    );
    let selected = id("item-4");
    let model = crate::host_panel_models::HostPanelModel {
        title: "Repositories".to_owned(),
        body: PanelBody::List(ListBody {
            items: (0..5)
                .map(|index| ListItem {
                    id: id(&format!("item-{index}")),
                    label: format!("Item {index}"),
                    description: None,
                    status: None,
                    count: None,
                    glyph: None,
                    badge: None,
                    suffix: None,
                    actions: Vec::new(),
                })
                .collect(),
            selected_id: Some(selected.clone()),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id: Some(selected),
        grabbed_id: None,
        scroll_offset: 0,
        reveal_selection: true,
    };

    super::project_host_model(&mut panel, model);

    assert_eq!(panel.content, content, "windowing must not change geometry");
    assert_eq!(panel.visible_window_origin, 2);
    assert_eq!(
        panel.lines,
        ["   Item 2", "   Item 3", ">> Item 4"],
        "the existing three-row window keeps item order and includes the selected row"
    );
    assert_eq!(
        panel.hit_targets,
        [
            Some(PanelHitTarget::ListItem(id("item-2"))),
            Some(PanelHitTarget::ListItem(id("item-3"))),
            Some(PanelHitTarget::ListItem(id("item-4"))),
        ]
    );
}

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
