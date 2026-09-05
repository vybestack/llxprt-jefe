#[test]
fn stale_model_shows_stale_marker_and_failed_status() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Empty],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Empty(EmptyBody {
                message: "first".to_owned(),
                action: None,
            }),
            BodyKind::Empty,
        ),
    );
    state
        .fail_runtime(panel)
        .unwrap_or_else(|error| panic!("runtime failure fixture: {error}"));
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert_eq!(main.status, PanelStatus::Failed);
    assert!(main.lines.iter().any(|l| l.contains("stale")));
}

// ---------------------------------------------------------------------------
// Affordance projection
// ---------------------------------------------------------------------------

#[test]
fn affordances_project_enabled_and_disabled_states() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Empty],
    );
    accept_snapshot(
        &mut state,
        panel,
        PanelSnapshot {
            model_schema: MODEL_SCHEMA,
            panel_instance_id: panel.as_u64(),
            generation: 1,
            revision: 1,
            kind: BodyKind::Empty,
            title: "t".to_owned(),
            description: None,
            loading: false,
            action_affordances: vec![
                Affordance {
                    id: id("open"),
                    label: "Open".to_owned(),
                    action_id: action_id("vendor.run"),
                    arguments: None,
                    enabled: true,
                    unavailable_reason: None,
                },
                Affordance {
                    id: id("delete"),
                    label: "Delete".to_owned(),
                    action_id: action_id("vendor.run"),
                    arguments: None,
                    enabled: false,
                    unavailable_reason: Some("read only".to_owned()),
                },
            ],
            body: PanelBody::Empty(EmptyBody {
                message: "m".to_owned(),
                action: None,
            }),
        },
    );
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert!(main.lines.iter().any(|l| l.contains("[open] Open")));
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("[delete] Delete (unavailable: read only)"))
    );
}

#[test]
fn projected_rows_carry_matching_semantic_and_unavailable_hit_targets() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    let item_id = id("alpha");
    accept_snapshot(
        &mut state,
        panel,
        list_snapshot(
            panel,
            vec![list_item("alpha", "Alpha", None, &["open"])],
            "alpha",
            vec![
                affordance("open", "Open", true, None),
                affordance("blocked", "Blocked", false, Some("not now")),
            ],
            Some("next"),
        ),
    );

    let view = project_view(&descriptor, &state, &layout);
    let main = projected_panel(&view, "main");
    assert_eq!(main.hit_targets.len(), main.lines.len());
    let target_for = |needle: &str| {
        main.lines
            .iter()
            .position(|line| line.contains(needle))
            .and_then(|index| main.hit_targets[index].as_ref())
    };
    assert_eq!(
        target_for(">> Alpha"),
        Some(&PanelHitTarget::ListItem(item_id))
    );
    assert_eq!(
        target_for("more results available"),
        Some(&PanelHitTarget::PageRequested)
    );
    assert_eq!(
        target_for("[open] Open"),
        Some(&PanelHitTarget::Action(id("open")))
    );
    assert_eq!(
        target_for("[blocked] Blocked"),
        Some(&PanelHitTarget::Unavailable)
    );
}

#[test]
fn semantic_targets_are_bound_to_source_rows_not_recovered_from_labels() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    let first = id("first");
    let second = id("second");
    accept_snapshot(
        &mut state,
        panel,
        list_snapshot(
            panel,
            vec![
                list_item("first", "A", Some("same"), &["open"]),
                list_item("second", "AB", Some("same"), &["edit"]),
            ],
            "first",
            vec![
                affordance("open", "Open", true, None),
                affordance("edit", "Edit", true, None),
            ],
            None,
        ),
    );

    let view = project_view(&descriptor, &state, &layout);
    let main = projected_panel(&view, "main");
    assert_eq!(
        exact_target(main, ">> A"),
        Some(&PanelHitTarget::ListItem(first))
    );
    assert_eq!(
        exact_target(main, "   AB"),
        Some(&PanelHitTarget::ListItem(second))
    );
    assert_eq!(
        exact_target(main, "   actions: open"),
        Some(&PanelHitTarget::Action(id("open")))
    );
    assert_eq!(
        exact_target(main, "   actions: edit"),
        Some(&PanelHitTarget::Action(id("edit")))
    );
    let duplicate_descriptions = main
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.as_str() == "   same")
        .map(|(index, _)| main.hit_targets[index].as_ref())
        .collect::<Vec<_>>();
    assert_eq!(
        duplicate_descriptions,
        vec![
            Some(&PanelHitTarget::ListItem(id("first"))),
            Some(&PanelHitTarget::ListItem(id("second"))),
        ]
    );
}

#[test]
fn wrapped_and_scrolled_controls_retain_structural_hit_targets() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 24, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    let item = id("long-item");
    accept_snapshot(
        &mut state,
        panel,
        list_snapshot(
            panel,
            vec![list_item(
                "long-item",
                "A long interactive item label that wraps",
                Some("A long interactive description that wraps too"),
                &["blocked"],
            )],
            "long-item",
            vec![affordance(
                "blocked",
                "Blocked control",
                false,
                Some("not now"),
            )],
            None,
        ),
    );

    let initial = project_view(&descriptor, &state, &layout);
    let main = projected_panel(&initial, "main");
    assert!(main.max_scroll_offset > 0);
    assert!(
        main.hit_targets
            .iter()
            .filter(|target| target.as_ref() == Some(&PanelHitTarget::ListItem(item.clone())))
            .count()
            > 1,
        "every wrapped item row must remain interactive"
    );

    state
        .update_host_local(
            panel,
            HostLocal {
                scroll_offset: main.max_scroll_offset,
                ..HostLocal::default()
            },
        )
        .unwrap_or_else(|error| panic!("host local: {error:?}"));
    let scrolled = project_view(&descriptor, &state, &layout);
    assert!(
        projected_panel(&scrolled, "main")
            .hit_targets
            .iter()
            .any(|target| target.as_ref() == Some(&PanelHitTarget::Unavailable)),
        "a clipped disabled embedded control must remain explicitly unavailable"
    );
}

#[test]
fn projection_clamps_scroll_metadata_to_the_wrapped_content() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 30, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Detail],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Detail(DetailBody {
                document: "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty"
                    .to_owned(),
                metadata: vec![DetailMetadata {
                    label: "State".to_owned(),
                    value: "Ready".to_owned(),
                }],
                actions: Vec::new(),
            }),
            BodyKind::Detail,
        ),
    );

    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert!(main.max_scroll_offset > 0);
    assert!(main.lines.len() <= usize::from(main.content.height));
    assert_eq!(main.hit_targets.len(), main.lines.len());
}

#[test]
fn projection_repairs_a_removed_host_selection_to_the_current_model() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    let alpha = id("alpha");
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::List(ListBody {
                items: vec![ListItem {
                    id: alpha.clone(),
                    label: "Alpha".to_owned(),
                    description: None,
                    status: None,
                    count: None,
                    actions: Vec::new(),
                }],
                selected_id: Some(alpha),
                next_page_token: None,
            }),
            BodyKind::List,
        ),
    );
    state
        .update_host_local(
            panel,
            HostLocal {
                selected_id: Some(id("removed")),
                ..HostLocal::default()
            },
        )
        .unwrap_or_else(|error| panic!("host local: {error:?}"));

    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    assert!(view.panels[0].lines.iter().any(|line| line == ">> Alpha"));
}
