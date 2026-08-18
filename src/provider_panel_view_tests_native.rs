fn assert_native_selectable_row(
    body: PanelBody,
    kind: BodyKind,
    expected_line: &str,
    expected_target: PanelHitTarget,
) {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[kind],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(panel, 1, body, kind),
    );

    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    assert_eq!(view.panels[0].lines, [expected_line]);
    assert_eq!(view.panels[0].hit_targets, [Some(expected_target)]);
}

#[test]
fn tree_and_structured_diff_render_native_selectable_rows() {
    for (body, kind, expected_line, expected_target) in [
        (
            PanelBody::Tree(TreeBody {
                schema_version: 1,
                nodes: vec![TreeNode {
                    id: id("vendor.node"),
                    parent_id: None,
                    label: "Tree node".to_owned(),
                    semantic_key: id("tree-node"),
                    depth: 0,
                    expandable: false,
                    expanded: false,
                }],
                selected_id: Some(id("vendor.node")),
            }),
            BodyKind::Tree,
            ">   Tree node",
            PanelHitTarget::TreeNode(id("vendor.node")),
        ),
        (
            PanelBody::StructuredDiff(StructuredDiffBody {
                schema_version: 1,
                files: vec![StructuredDiffFile {
                    id: id("vendor.file"),
                    old_path: None,
                    new_path: Some("src/new.rs".to_owned()),
                    old_mode: None,
                    new_mode: None,
                    binary: true,
                    hunks: vec![],
                }],
                selected_file_id: Some(id("vendor.file")),
            }),
            BodyKind::StructuredDiff,
            ">> src/new.rs [binary]",
            PanelHitTarget::DiffFile(id("vendor.file")),
        ),
    ] {
        assert_native_selectable_row(body, kind, expected_line, expected_target);
    }
}
