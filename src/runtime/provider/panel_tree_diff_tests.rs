// Issue #705 S1: exact Tree and StructuredDiff provider contracts.

fn tree_body(nodes: &str, selected_id: Option<&str>) -> String {
    let selected = selected_id.map_or_else(String::new, |id| {
        let mut field = String::from(",\"selected_id\":\"");
        field.push_str(id);
        field.push('"');
        field
    });
    format!(r#"{{"schema_version":1,"nodes":[{nodes}]{selected}}}"#)
}

fn root_node(extra: &str) -> String {
    format!(
        r#"{{"id":"vendor.root","label":"Root","semantic_key":"root","depth":0,"expandable":true,"expanded":true{extra}}}"#
    )
}

fn text_diff_file(hunks: &str) -> String {
    format!(
        r#"{{"id":"vendor.file","old_path":"a/file.rs","new_path":"b/file.rs","old_mode":"100644","new_mode":"100644","binary":false,"hunks":[{hunks}]}}"#
    )
}

fn diff_body(files: &str, selected_file_id: Option<&str>) -> String {
    let selected = selected_file_id.map_or_else(String::new, |id| {
        let mut field = String::from(",\"selected_file_id\":\"");
        field.push_str(id);
        field.push('"');
        field
    });
    format!(r#"{{"schema_version":1,"files":[{files}]{selected}}}"#)
}

fn one_hunk() -> &'static str {
    r#"{"header":"@@ -1,2 +1,2 @@","old_start":1,"old_lines":2,"new_start":1,"new_lines":2,"lines":[{"origin":"context","old_line":1,"new_line":1,"content":"same","no_newline":false},{"origin":"removed","old_line":2,"content":"old","no_newline":false},{"origin":"added","new_line":2,"content":"new","no_newline":true}]}"#
}

#[test]
fn tree_body_parses_the_exact_versioned_dto() {
    let nodes = format!(
        r#"{},{{"id":"vendor.child","parent_id":"vendor.root","label":"Child","semantic_key":"child","depth":1,"expandable":false,"expanded":false}}"#,
        root_node("")
    );
    let snapshot = parse_snapshot("tree", &tree_body(&nodes, Some("vendor.child")));
    let PanelBody::Tree(tree) = snapshot.body else {
        panic!("expected Tree body");
    };
    assert_eq!(tree.schema_version, 1);
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(tree.nodes[1].parent_id.as_ref().map(Id::as_str), Some("vendor.root"));
    assert_eq!(tree.selected_id.as_ref().map(Id::as_str), Some("vendor.child"));
}

#[test]
fn structured_diff_line_origins_use_the_exact_closed_vocabulary() {
    assert_eq!(
        DiffLineOrigin::ALL.map(DiffLineOrigin::as_str),
        ["context", "added", "removed"]
    );
    assert_eq!(DiffLineOrigin::from_wire("Added"), None);
    assert_eq!(DiffLineOrigin::from_wire("changed"), None);
}

#[test]
fn structured_diff_body_parses_the_exact_versioned_dto() {
    let file = text_diff_file(one_hunk());
    let snapshot = parse_snapshot(
        "structured-diff",
        &diff_body(&file, Some("vendor.file")),
    );
    let PanelBody::StructuredDiff(diff) = snapshot.body else {
        panic!("expected StructuredDiff body");
    };
    assert_eq!(diff.schema_version, 1);
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].hunks[0].lines.len(), 3);
}

#[test]
fn tree_and_diff_reject_unknown_versions_and_fields() {
    for (kind, body) in [
        (
            "tree",
            r#"{"schema_version":2,"nodes":[]}"#.to_owned(),
        ),
        (
            "tree",
            r#"{"schema_version":1,"nodes":[],"unknown":true}"#.to_owned(),
        ),
        (
            "structured-diff",
            r#"{"schema_version":2,"files":[]}"#.to_owned(),
        ),
        (
            "structured-diff",
            r#"{"schema_version":1,"files":[],"unknown":true}"#.to_owned(),
        ),
    ] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body(kind, &body),
        );
        assert!(
            matches!(
                rejected(&bytes, Direction::ProviderToHost),
                ProviderError::InvalidValue { .. } | ProviderError::UnknownField { .. }
            ),
            "{kind} must reject {body}"
        );
    }
}

#[test]
fn tree_requires_parent_before_child_and_exact_depth() {
    let child = r#"{"id":"vendor.child","parent_id":"vendor.root","label":"Child","semantic_key":"child","depth":1,"expandable":false,"expanded":false}"#;
    for nodes in [
        format!("{child},{}", root_node("")),
        format!(
            "{},{}",
            root_node(""),
            child.replace(r#"depth":1"#, r#"depth":2"#)
        ),
    ] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body("tree", &tree_body(&nodes, None)),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

#[test]
fn tree_rejects_duplicate_ids_semantic_keys_and_expanded_leaves() {
    let duplicate_id = format!("{},{}", root_node(""), root_node(""));
    let duplicate_key = format!(
        r#"{},{{"id":"vendor.other","label":"Other","semantic_key":"root","depth":0,"expandable":false,"expanded":false}}"#,
        root_node("")
    );
    let expanded_leaf = r#"{"id":"vendor.leaf","label":"Leaf","semantic_key":"leaf","depth":0,"expandable":false,"expanded":true}"#.to_owned();
    for nodes in [duplicate_id, duplicate_key, expanded_leaf] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body("tree", &tree_body(&nodes, None)),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

#[test]
fn tree_selected_id_must_exist() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "tree",
            &tree_body(&root_node(""), Some("vendor.missing")),
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn tree_node_limit_is_inclusive() {
    let nodes = (0..1000)
        .map(|index| {
            format!(
                r#"{{"id":"vendor.n{index}","label":"Node","semantic_key":"key-{index}","depth":0,"expandable":false,"expanded":false}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let at_limit = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("tree", &tree_body(&nodes, None)),
    );
    assert!(parse_message(&at_limit, Direction::ProviderToHost).is_ok());

    let over = format!(
        r#"{nodes},{{"id":"vendor.over","label":"Over","semantic_key":"over","depth":0,"expandable":false,"expanded":false}}"#
    );
    let over_limit = envelope(
        "panel-snapshot",
        "p-000002",
        1,
        &snapshot_body("tree", &tree_body(&over, None)),
    );
    assert!(matches!(
        rejected(&over_limit, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn structured_diff_rejects_binary_hunks_and_missing_paths() {
    let binary_with_hunk = format!(
        r#"{{"id":"vendor.bin","old_path":"a/bin","new_path":"b/bin","binary":true,"hunks":[{}]}}"#,
        one_hunk()
    );
    let no_paths = r#"{"id":"vendor.none","binary":false,"hunks":[]}"#;
    for file in [binary_with_hunk.as_str(), no_paths] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body("structured-diff", &diff_body(file, None)),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

#[test]
fn structured_diff_line_origins_require_exact_line_number_sides() {
    for line in [
        r#"{"origin":"context","old_line":1,"content":"bad","no_newline":false}"#,
        r#"{"origin":"added","old_line":1,"new_line":1,"content":"bad","no_newline":false}"#,
        r#"{"origin":"removed","new_line":1,"content":"bad","no_newline":false}"#,
    ] {
        let hunk = format!(
            r#"{{"header":"@@ -1,1 +1,1 @@","old_start":1,"old_lines":1,"new_start":1,"new_lines":1,"lines":[{line}]}}"#
        );
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body(
                "structured-diff",
                &diff_body(&text_diff_file(&hunk), None),
            ),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

#[test]
fn structured_diff_rejects_empty_hunks() {
    let empty_hunk =
        r#"{"header":"empty","old_start":0,"old_lines":0,"new_start":0,"new_lines":0,"lines":[]}"#;
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "structured-diff",
            &diff_body(&text_diff_file(empty_hunk), None),
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn structured_diff_validates_hunk_ranges_and_order() {
    let bad_count = one_hunk().replace(r#"old_lines":2"#, r#"old_lines":3"#);
    let second_before_first = r#"{"header":"late","old_start":0,"old_lines":0,"new_start":10,"new_lines":1,"lines":[{"origin":"added","new_line":10,"content":"late","no_newline":false}]},{"header":"early","old_start":0,"old_lines":0,"new_start":5,"new_lines":1,"lines":[{"origin":"added","new_line":5,"content":"early","no_newline":false}]}"#.to_owned();
    for hunks in [bad_count, second_before_first] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body(
                "structured-diff",
                &diff_body(&text_diff_file(&hunks), None),
            ),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

#[test]
fn structured_diff_selected_file_must_exist_and_file_ids_are_unique() {
    let file = text_diff_file("");
    for body in [
        diff_body(&file, Some("vendor.missing")),
        diff_body(&format!("{file},{file}"), None),
    ] {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body("structured-diff", &body),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::InvalidValue { .. }
        ));
    }
}

fn assert_nested_bodies_rejected(invalid_bodies: &[(&str, String)]) {
    for (kind, body) in invalid_bodies {
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body(kind, body),
        );
        assert!(matches!(
            rejected(&bytes, Direction::ProviderToHost),
            ProviderError::UnknownField { .. } | ProviderError::TypeMismatch { .. }
        ));
    }
}

#[test]
fn tree_nodes_reject_unknown_fields_and_wrong_types() {
    assert_nested_bodies_rejected(&[
        (
            "tree",
            tree_body(
                r#"{"id":"vendor.node","label":"Node","semantic_key":"node","depth":0,"expandable":false,"expanded":false,"unknown":true}"#,
                None,
            ),
        ),
        (
            "tree",
            tree_body(
                r#"{"id":"vendor.node","label":"Node","semantic_key":"node","depth":"0","expandable":false,"expanded":false}"#,
                None,
            ),
        ),
    ]);
}

#[test]
fn structured_diff_nested_objects_reject_unknown_fields_and_wrong_types() {
    assert_nested_bodies_rejected(&[
        (
            "structured-diff",
            diff_body(
                r#"{"id":"vendor.file","new_path":"b/file","binary":true,"hunks":[],"unknown":true}"#,
                None,
            ),
        ),
        (
            "structured-diff",
            diff_body(
                r#"{"id":"vendor.file","new_path":"b/file","binary":"true","hunks":[]}"#,
                None,
            ),
        ),
        (
            "structured-diff",
            diff_body(
                &text_diff_file(
                    r#"{"header":"h","old_start":0,"old_lines":0,"new_start":0,"new_lines":0,"lines":[],"unknown":true}"#,
                ),
                None,
            ),
        ),
        (
            "structured-diff",
            diff_body(
                &text_diff_file(
                    r#"{"header":"h","old_start":"0","old_lines":0,"new_start":0,"new_lines":0,"lines":[]}"#,
                ),
                None,
            ),
        ),
        (
            "structured-diff",
            diff_body(
                &text_diff_file(
                    r#"{"header":"h","old_start":1,"old_lines":1,"new_start":1,"new_lines":1,"lines":[{"origin":"context","old_line":1,"new_line":1,"content":"line","no_newline":false,"unknown":true}]}"#,
                ),
                None,
            ),
        ),
        (
            "structured-diff",
            diff_body(
                &text_diff_file(
                    r#"{"header":"h","old_start":1,"old_lines":1,"new_start":1,"new_lines":1,"lines":[{"origin":"context","old_line":1,"new_line":1,"content":"line","no_newline":"false"}]}"#,
                ),
                None,
            ),
        ),
    ]);
}


#[test]
fn structured_diff_file_limit_is_inclusive() {
    let files = (0..256)
        .map(|index| {
            format!(
                r#"{{"id":"vendor.file-{index}","new_path":"b/file-{index}","binary":true,"hunks":[]}}"#
            )
        })
        .collect::<Vec<_>>();
    let at_limit = diff_body(&files.join(","), None);
    parse_snapshot("structured-diff", &at_limit);

    let over_limit = format!(
        r#"{},{{"id":"vendor.file-over","new_path":"b/file-over","binary":true,"hunks":[]}}"#,
        files.join(",")
    );
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("structured-diff", &diff_body(&over_limit, None)),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn structured_diff_hunk_limit_is_inclusive() {
    let hunks = (1..=1024)
        .map(|index| {
            format!(
                r#"{{"header":"h{index}","old_start":0,"old_lines":0,"new_start":{index},"new_lines":1,"lines":[{{"origin":"added","new_line":{index},"content":"x","no_newline":false}}]}}"#
            )
        })
        .collect::<Vec<_>>();
    let at_limit = diff_body(&text_diff_file(&hunks.join(",")), None);
    parse_snapshot("structured-diff", &at_limit);

    let over_limit = format!(
        r#"{},{{"header":"over","old_start":0,"old_lines":0,"new_start":1025,"new_lines":1,"lines":[{{"origin":"added","new_line":1025,"content":"x","no_newline":false}}]}}"#,
        hunks.join(",")
    );
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "structured-diff",
            &diff_body(&text_diff_file(&over_limit), None),
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::Json(BoundedJsonError::ArrayTooLarge { limit: 1024 })
    ));
}

#[test]
fn structured_diff_line_count_and_content_byte_limits_are_inclusive() {
    let lines = (1..=1024)
        .map(|line| {
            format!(
                r#"{{"origin":"added","new_line":{line},"content":"x","no_newline":false}}"#
            )
        })
        .collect::<Vec<_>>();
    let hunk = format!(
        r#"{{"header":"h","old_start":0,"old_lines":0,"new_start":1,"new_lines":1024,"lines":[{}]}}"#,
        lines.join(",")
    );
    parse_snapshot(
        "structured-diff",
        &diff_body(&text_diff_file(&hunk), None),
    );

    let over_lines = format!(
        r#"{},{{"origin":"added","new_line":{},"content":"x","no_newline":false}}"#,
        lines.join(","),
        1024 + 1
    );
    let over_hunk = format!(
        r#"{{"header":"h","old_start":0,"old_lines":0,"new_start":1,"new_lines":{},"lines":[{over_lines}]}}"#,
        1024 + 1
    );
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "structured-diff",
            &diff_body(&text_diff_file(&over_hunk), None),
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::Json(BoundedJsonError::ArrayTooLarge { limit: 1024 })
    ));

    for (size, accepted) in [
        (262_144, true),
        (262_144 + 1, false),
    ] {
        let content = "x".repeat(size);
        let hunk = format!(
            r#"{{"header":"h","old_start":0,"old_lines":0,"new_start":1,"new_lines":1,"lines":[{{"origin":"added","new_line":1,"content":"{content}","no_newline":false}}]}}"#
        );
        let bytes = envelope(
            "panel-snapshot",
            "p-000001",
            1,
            &snapshot_body(
                "structured-diff",
                &diff_body(&text_diff_file(&hunk), None),
            ),
        );
        assert_eq!(
            parse_message(&bytes, Direction::ProviderToHost).is_ok(),
            accepted
        );
    }
}

#[test]
fn expansion_changed_event_is_closed_and_round_trips() {
    let event = parse_event(
        r#"{"kind":"expansion-changed","id":"vendor.node","expanded":true}"#,
    );
    assert!(matches!(
        event,
        PanelEvent::ExpansionChanged { id, expanded }
            if id.as_str() == "vendor.node" && expanded
    ));

    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        r#"{"panel_instance_id":1,"generation":1,"revision":1,"event":{"kind":"expansion-changed","id":"vendor.node","expanded":true,"unknown":1}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}