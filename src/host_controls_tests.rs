use crate::domain::{
    Id, TypedMap, TypedValue,
    action_registry::ActionId,
    plugin::{
        ModelKind,
        field::{Field, FieldDraft, FieldKind, RestartScope},
    },
};
use crate::host_controls::{
    ControlAction, ControlIntent, ControlKind, PanelHitTarget, control_intent, project_control,
    public_factory,
};
use crate::runtime::provider::protocol::{
    Affordance, BodyKind, DiffLineOrigin, ErrorBody, FormBody, ListBody, ListItem, PanelBody,
    PanelEvent, PanelSnapshot, ProgressBody, StructuredDiffBody, StructuredDiffFile,
    StructuredDiffHunk, StructuredDiffLine, StructuredDiffPath, TreeBody, TreeNode,
};
use unicode_width::UnicodeWidthStr;

#[test]
fn control_kind_is_the_exact_nine_value_public_vocabulary() {
    assert_eq!(
        ControlKind::ALL.map(ControlKind::as_wire),
        [
            "list",
            "tree",
            "detail",
            "structured-diff",
            "form",
            "status",
            "progress",
            "empty",
            "error",
        ]
    );
    for (model, body) in ModelKind::ALL.into_iter().zip(BodyKind::ALL) {
        let expected = ControlKind::from(model);
        assert_eq!(ControlKind::from(body), expected);
        assert_eq!(ControlKind::from_wire(expected.as_wire()), Some(expected));
    }
    assert_eq!(ControlKind::from_wire("terminal"), None);
    assert_eq!(ControlKind::from_wire("Tree"), None);
}

#[test]
fn host_control_dispatch_is_exhaustive_over_the_public_vocabulary() {
    for kind in ControlKind::ALL {
        assert_eq!(public_factory(kind).kind(), kind);
    }
}

#[test]
fn terminal_is_not_a_public_control_kind() {
    assert_eq!(ControlKind::ALL.len(), 9);
    assert_eq!(ControlKind::from_wire("terminal"), None);
}

/// Issue #723 fix 4: a sidebar row is one line — `name [N]` truncates the
/// name to the pane width so the count survives and the row never wraps.
#[test]
fn list_label_rows_truncate_instead_of_wrapping() {
    let long_name = "a".repeat(30);
    let snapshot = snapshot(PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("item"),
            label: long_name,
            description: None,
            status: Some("1".to_owned()),
            count: None,
            actions: Vec::new(),
        }],
        selected_id: None,
        next_page_token: None,
    }));

    let rows = project_control(&snapshot, None, None, 12);

    assert_eq!(
        rows.len(),
        1,
        "the row must never wrap into a second row: {rows:?}"
    );
    let row = rows
        .first()
        .map(|row| row.text.as_str())
        .unwrap_or_default();
    assert!(row.starts_with(">> "), "the marker survives: {row:?}");
    assert!(row.ends_with(" [1]"), "the agent count survives: {row:?}");
    assert!(
        row.chars().count() <= 12,
        "the row fits the pane width: {row:?}"
    );
    assert!(
        row.contains('…'),
        "the overlong name is visibly truncated: {row:?}"
    );
}

/// #745 follow-up B5: a typed count is a protected suffix, not part of the
/// truncatable label. The pane spends what it has on the label and always
/// keeps the count, because a row that reads `Needs you (1…` states a number
/// that is not the number.
#[test]
fn list_count_survives_when_the_label_does_not() {
    let snapshot = snapshot(PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("item"),
            label: "a".repeat(30),
            description: None,
            status: None,
            count: Some(12),
            actions: Vec::new(),
        }],
        selected_id: None,
        next_page_token: None,
    }));

    let rows = project_control(&snapshot, None, None, 12);

    assert_eq!(
        rows.len(),
        1,
        "the row must never wrap into a second row: {rows:?}"
    );
    let row = rows
        .first()
        .map(|row| row.text.as_str())
        .unwrap_or_default();
    assert!(row.starts_with(">> "), "the marker survives: {row:?}");
    assert!(row.ends_with(" (12)"), "the count survives whole: {row:?}");
    assert!(
        UnicodeWidthStr::width(row) <= 12,
        "the row fits the pane width: {row:?}"
    );
    assert!(
        row.contains('…'),
        "the label, not the count, is the elided part: {row:?}"
    );
}

/// #745 follow-up B7: an item may carry both, so the composition order is
/// pinned rather than left to whichever suffix is appended last. The count
/// belongs to the name phrase and leads; the status word trails.
#[test]
fn list_renders_a_count_before_a_status_suffix() {
    let snapshot = snapshot(PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("item"),
            label: "Alpha".to_owned(),
            description: None,
            status: Some("Running".to_owned()),
            count: Some(2),
            actions: Vec::new(),
        }],
        selected_id: None,
        next_page_token: None,
    }));

    let rows = project_control(&snapshot, None, None, 40);

    assert_eq!(
        rows.first().map(|row| row.text.as_str()),
        Some(">> Alpha (2) [Running]"),
        "count then status, both after the label: {rows:?}"
    );
}

/// #745 follow-up B5, boundary: when the marker, the label and the count
/// cannot share the row, the marker and the label go and the count stays
/// whole. The row is still exactly one row and still fits the pane.
#[test]
fn a_count_too_wide_to_share_the_row_is_kept_whole_without_the_label() {
    let rows = extreme_width_rows("Needs you", Some(1234), None, 6);

    assert_eq!(
        rows,
        vec!["(1234)".to_owned()],
        "the marker and the label are sacrificed, not the number"
    );
}

/// #745 follow-up B5, boundary: below the width of the count itself there is
/// no honest way to show it, so it is dropped whole. A row reading `(1…`
/// states a count of one, which is the #745 defect in miniature.
#[test]
fn a_count_that_cannot_fit_whole_is_dropped_rather_than_sliced() {
    let rows = extreme_width_rows("Needs you", Some(1234), None, 5);

    assert_eq!(
        rows,
        vec![">> N…".to_owned()],
        "no fragment of the count is painted"
    );
    assert_no_partial_tokens(&rows);
}

/// #745 follow-up B7, boundary: the status word obeys the same rule. `[Runn…`
/// names a status no agent is in, so the whole suffix goes and the count —
/// which still fits — stays.
#[test]
fn a_status_word_too_wide_for_the_row_is_dropped_whole() {
    let rows = extreme_width_rows("Alpha", Some(2), Some("Running"), 14);

    assert_eq!(
        rows,
        vec![">> Alpha (2)".to_owned()],
        "the status is dropped whole and the count is untouched"
    );
    assert_no_partial_tokens(&rows);
}

/// #745 follow-up B7, boundary: with the count too wide to share the row and
/// the status narrow enough, the count is the suffix dropped whole. The label
/// stays, because it is the one span an elision may touch.
#[test]
fn a_count_too_wide_to_share_the_row_yields_to_a_status_that_fits() {
    let rows = extreme_width_rows("Alpha", Some(1_234_567), Some("ok"), 10);

    assert_eq!(
        rows,
        vec![">> A… [ok]".to_owned()],
        "the label carries the ellipsis; the status is whole and the count absent"
    );
    assert_no_partial_tokens(&rows);
}

/// #745 follow-up B5, boundary: at a width that fits nothing semantic the row
/// is the elided label alone, never a fragment of a number or a status word.
#[test]
fn a_row_narrower_than_every_suffix_paints_only_the_elided_label() {
    let rows = extreme_width_rows("Needs you", Some(1234), Some("Running"), 4);

    assert_eq!(
        rows,
        vec![">> …".to_owned()],
        "the marker and an elided label, nothing semantic"
    );
    assert_no_partial_tokens(&rows);
}

/// One list item projected at a width narrower than the shipped panes, as row
/// text. The shared control must return exactly one row whatever the width, so
/// asserting the whole vector proves that too.
fn extreme_width_rows(
    label: &str,
    count: Option<usize>,
    status: Option<&str>,
    width: usize,
) -> Vec<String> {
    let snapshot = snapshot(PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("item"),
            label: label.to_owned(),
            description: None,
            status: status.map(str::to_owned),
            count,
            actions: Vec::new(),
        }],
        selected_id: None,
        next_page_token: None,
    }));

    let rows: Vec<String> = project_control(&snapshot, None, None, width)
        .into_iter()
        .map(|row| row.text)
        .collect();
    for row in &rows {
        assert!(
            UnicodeWidthStr::width(row.as_str()) <= width,
            "every row fits the pane width {width}: {row:?}"
        );
    }
    rows
}

/// No row carries the opening half of a count or of a status word: an ellipsis
/// may only follow label text.
fn assert_no_partial_tokens(rows: &[String]) {
    for row in rows {
        assert!(
            !row.contains('(') || row.contains(')'),
            "a count is painted whole or not at all: {row:?}"
        );
        assert!(
            !row.contains('[') || row.contains(']'),
            "a status word is painted whole or not at all: {row:?}"
        );
    }
}

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid test id `{value}`: {error}"))
}

fn string_field(value: &str, label: &str) -> Field {
    Field::parse(FieldDraft {
        id: id(value),
        label: label.to_owned(),
        description: None,
        kind: FieldKind::String,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| unreachable!("valid form field fixture: {error}"))
}

fn snapshot(body: PanelBody) -> PanelSnapshot {
    PanelSnapshot {
        model_schema: 1,
        panel_instance_id: 1,
        generation: 1,
        revision: 1,
        kind: body.kind(),
        title: "Fixture".to_owned(),
        description: None,
        loading: false,
        action_affordances: Vec::new(),
        body,
    }
}

#[test]
fn factory_actions_preserve_declared_arguments_and_reject_mismatched_semantics() {
    let mut arguments = TypedMap::new();
    arguments.insert(id("force"), TypedValue::Bool(true));
    let mut snapshot = snapshot(PanelBody::List(ListBody {
        items: Vec::new(),
        selected_id: None,
        next_page_token: None,
    }));
    snapshot.action_affordances.push(Affordance {
        id: id("open"),
        label: "Open".to_owned(),
        action_id: ActionId::parse("vendor.open")
            .unwrap_or_else(|error| unreachable!("valid action id: {error}")),
        arguments: Some(arguments.clone()),
        enabled: true,
        unavailable_reason: None,
    });

    assert_eq!(
        control_intent(
            &snapshot,
            None,
            None,
            None,
            ControlAction::Action(id("open")),
        ),
        ControlIntent::Event(PanelEvent::Action {
            id: id("open"),
            arguments,
        })
    );
    for action in [
        ControlAction::Action(id("missing")),
        ControlAction::Select(id("missing")),
        ControlAction::Submit,
        ControlAction::Retry,
        ControlAction::Cancel,
        ControlAction::Link(id("missing")),
    ] {
        assert_eq!(
            control_intent(&snapshot, None, None, None, action),
            ControlIntent::None
        );
    }
}

fn assert_retry_actions(snapshot: &PanelSnapshot, expected: ControlIntent) {
    for action in [ControlAction::Retry, ControlAction::Activate] {
        assert_eq!(
            control_intent(snapshot, None, None, None, action),
            expected.clone()
        );
    }
}

#[test]
fn retry_and_cancel_require_live_body_capabilities_and_retry_affordance() {
    let retry_action = ActionId::parse("vendor.retry")
        .unwrap_or_else(|error| unreachable!("valid action id: {error}"));
    let mut error = snapshot(PanelBody::Error(ErrorBody {
        code: "vendor-failed".to_owned(),
        message: "Failed".to_owned(),
        retryable: true,
        retry_action: None,
    }));
    let mut progress = snapshot(PanelBody::Progress(ProgressBody {
        message: "Working".to_owned(),
        completed: Some(1),
        total: Some(2),
        cancellable: false,
    }));

    assert_retry_actions(&error, ControlIntent::None);
    let PanelBody::Error(body) = &mut error.body else {
        unreachable!("error fixture")
    };
    body.retry_action = Some(id("retry"));
    error.action_affordances.push(Affordance {
        id: id("other"),
        label: "Other".to_owned(),
        action_id: retry_action.clone(),
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    });
    assert_retry_actions(&error, ControlIntent::None);
    error.action_affordances[0].id = id("retry");
    error.action_affordances[0].enabled = false;
    assert_retry_actions(&error, ControlIntent::None);
    error.action_affordances[0].enabled = true;
    assert_retry_actions(&error, ControlIntent::Event(PanelEvent::Retry));

    assert_eq!(
        control_intent(&progress, None, None, None, ControlAction::Cancel),
        ControlIntent::None
    );
    let PanelBody::Progress(body) = &mut progress.body else {
        unreachable!("progress fixture")
    };
    body.cancellable = true;
    assert_eq!(
        control_intent(&progress, None, None, None, ControlAction::Cancel),
        ControlIntent::Event(PanelEvent::Cancel)
    );
}

#[test]
fn collapsed_tree_projection_omits_all_descendants_until_the_next_visible_sibling() {
    let snapshot = snapshot(PanelBody::Tree(TreeBody {
        schema_version: 1,
        nodes: vec![
            TreeNode {
                id: id("root"),
                parent_id: None,
                label: "Root".to_owned(),
                semantic_key: id("root-key"),
                depth: 0,
                expandable: true,
                expanded: false,
            },
            TreeNode {
                id: id("hidden-child"),
                parent_id: Some(id("root")),
                label: "Hidden child".to_owned(),
                semantic_key: id("hidden-child-key"),
                depth: 1,
                expandable: false,
                expanded: false,
            },
            TreeNode {
                id: id("next-root"),
                parent_id: None,
                label: "Next root".to_owned(),
                semantic_key: id("next-root-key"),
                depth: 0,
                expandable: false,
                expanded: false,
            },
        ],
        selected_id: Some(id("root")),
    }));

    let rows = project_control(&snapshot, None, None, 80);
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        ["> ▸ Root", "    Next root"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.target.clone())
            .collect::<Vec<_>>(),
        [
            Some(PanelHitTarget::TreeNode(id("root"))),
            Some(PanelHitTarget::TreeNode(id("next-root"))),
        ]
    );
}

#[test]
fn structured_diff_lines_clip_to_one_row_instead_of_wrapping() {
    let snapshot = snapshot(PanelBody::StructuredDiff(StructuredDiffBody {
        schema_version: 1,
        files: vec![StructuredDiffFile {
            id: id("file"),
            path: StructuredDiffPath::Renamed {
                old: "src/old.rs".to_owned(),
                new: "src/new.rs".to_owned(),
            },
            old_mode: None,
            new_mode: None,
            binary: false,
            hunks: vec![StructuredDiffHunk {
                header: "@@ -1 +1 @@".to_owned(),
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![StructuredDiffLine {
                    origin: DiffLineOrigin::Context,
                    old_line: Some(1),
                    new_line: Some(1),
                    content: "a line far wider than the available panel".to_owned(),
                    no_newline: false,
                }],
            }],
        }],
        selected_file_id: Some(id("file")),
    }));

    let rows = project_control(&snapshot, None, None, 16);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.text.chars().count() <= 16));
    assert_eq!(rows[0].target, Some(PanelHitTarget::DiffFile(id("file"))));
    assert!(rows[1..].iter().all(|row| row.target.is_none()));
}

#[test]
fn structured_diff_displays_every_typed_path_shape() {
    let paths = [
        StructuredDiffPath::Added("src/added.rs".to_owned()),
        StructuredDiffPath::Removed("src/removed.rs".to_owned()),
        StructuredDiffPath::Modified("src/modified.rs".to_owned()),
        StructuredDiffPath::Renamed {
            old: "src/old.rs".to_owned(),
            new: "src/new.rs".to_owned(),
        },
    ];
    let files = paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| StructuredDiffFile {
            id: id(&format!("file-{index}")),
            path,
            old_mode: None,
            new_mode: None,
            binary: true,
            hunks: Vec::new(),
        })
        .collect();
    let snapshot = snapshot(PanelBody::StructuredDiff(StructuredDiffBody {
        schema_version: 1,
        files,
        selected_file_id: Some(id("file-0")),
    }));

    let rows = project_control(&snapshot, None, None, 80);
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        [
            ">> src/added.rs [binary]",
            "   src/removed.rs [binary]",
            "   src/modified.rs [binary]",
            "   src/old.rs -> src/new.rs [binary]",
        ]
    );
}

#[test]
fn tree_intents_follow_visible_preorder_and_leave_expansion_authoritative() {
    let snapshot = snapshot(PanelBody::Tree(TreeBody {
        schema_version: 1,
        nodes: vec![
            TreeNode {
                id: id("root"),
                parent_id: None,
                label: "Root".to_owned(),
                semantic_key: id("root-key"),
                depth: 0,
                expandable: true,
                expanded: true,
            },
            TreeNode {
                id: id("leaf"),
                parent_id: Some(id("root")),
                label: "Leaf".to_owned(),
                semantic_key: id("leaf-key"),
                depth: 1,
                expandable: false,
                expanded: false,
            },
            TreeNode {
                id: id("next-root"),
                parent_id: None,
                label: "Next root".to_owned(),
                semantic_key: id("next-root-key"),
                depth: 0,
                expandable: false,
                expanded: false,
            },
        ],
        selected_id: Some(id("root")),
    }));

    assert_eq!(
        control_intent(&snapshot, None, None, None, ControlAction::Next),
        ControlIntent::Event(PanelEvent::Selected { id: id("leaf") })
    );
    assert_eq!(
        control_intent(&snapshot, None, None, None, ControlAction::Previous),
        ControlIntent::Event(PanelEvent::Selected {
            id: id("next-root"),
        })
    );
    assert_eq!(
        control_intent(&snapshot, None, None, None, ControlAction::Activate),
        ControlIntent::Event(PanelEvent::ExpansionChanged {
            id: id("root"),
            expanded: false,
        })
    );
    assert_eq!(
        control_intent(
            &snapshot,
            Some(&id("leaf")),
            None,
            None,
            ControlAction::Activate,
        ),
        ControlIntent::Event(PanelEvent::Activated { id: id("leaf") })
    );
}

#[test]
fn structured_diff_intents_select_and_activate_files_in_provider_order() {
    let file = |name: &str| StructuredDiffFile {
        id: id(name),
        path: StructuredDiffPath::Renamed {
            old: format!("old/{name}"),
            new: format!("new/{name}"),
        },

        old_mode: None,
        new_mode: None,
        binary: true,
        hunks: Vec::new(),
    };
    let snapshot = snapshot(PanelBody::StructuredDiff(StructuredDiffBody {
        schema_version: 1,
        files: vec![file("alpha"), file("beta")],
        selected_file_id: Some(id("alpha")),
    }));

    assert_eq!(
        control_intent(&snapshot, None, None, None, ControlAction::Next),
        ControlIntent::Event(PanelEvent::Selected { id: id("beta") })
    );
    assert_eq!(
        control_intent(
            &snapshot,
            Some(&id("beta")),
            None,
            None,
            ControlAction::Activate,
        ),
        ControlIntent::Event(PanelEvent::Activated { id: id("beta") })
    );
}

#[test]
fn activation_repairs_stale_local_list_and_diff_selection_to_the_visible_row() {
    let list = snapshot(PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("current"),
            label: "Current".to_owned(),
            description: None,
            status: None,
            count: None,
            actions: Vec::new(),
        }],
        selected_id: Some(id("current")),
        next_page_token: None,
    }));
    assert_eq!(
        control_intent(
            &list,
            Some(&id("removed")),
            None,
            None,
            ControlAction::Activate,
        ),
        ControlIntent::Event(PanelEvent::Activated { id: id("current") })
    );

    let diff = snapshot(PanelBody::StructuredDiff(StructuredDiffBody {
        schema_version: 1,
        files: vec![StructuredDiffFile {
            id: id("current-file"),
            path: StructuredDiffPath::Renamed {
                old: "old".to_owned(),
                new: "new".to_owned(),
            },
            old_mode: None,
            new_mode: None,
            binary: true,
            hunks: Vec::new(),
        }],
        selected_file_id: Some(id("current-file")),
    }));
    assert_eq!(
        control_intent(
            &diff,
            Some(&id("removed-file")),
            None,
            None,
            ControlAction::Activate,
        ),
        ControlIntent::Event(PanelEvent::Activated {
            id: id("current-file"),
        })
    );
}

#[test]
fn untouched_form_submits_the_displayed_values_only_when_submit_is_enabled() {
    let mut values = TypedMap::new();
    values.insert(id("query"), TypedValue::String("visible".to_owned()));
    let submit_action = ActionId::parse("vendor.submit")
        .unwrap_or_else(|error| unreachable!("valid action id: {error}"));
    let mut form = snapshot(PanelBody::Form(FormBody {
        fields: vec![string_field("query", "Query")],
        values: values.clone(),
        field_errors: Vec::new(),
        submit_action: submit_action.clone(),
    }));
    assert_eq!(
        control_intent(&form, None, None, None, ControlAction::Submit),
        ControlIntent::None
    );

    form.action_affordances.push(Affordance {
        id: id("submit"),
        label: "Submit".to_owned(),
        action_id: submit_action,
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    });
    assert_eq!(
        control_intent(&form, None, None, None, ControlAction::Submit),
        ControlIntent::Event(PanelEvent::Submit { values })
    );
}

#[test]
fn form_submission_matches_current_fields_with_partial_draft_overrides() {
    let mut values = TypedMap::new();
    values.insert(id("query"), TypedValue::String("provider query".to_owned()));
    values.insert(id("scope"), TypedValue::String("provider scope".to_owned()));
    let submit_action = ActionId::parse("vendor.submit")
        .unwrap_or_else(|error| unreachable!("valid action id: {error}"));
    let mut form = snapshot(PanelBody::Form(FormBody {
        fields: vec![
            string_field("query", "Query"),
            string_field("scope", "Scope"),
        ],
        values,
        field_errors: Vec::new(),
        submit_action: submit_action.clone(),
    }));
    form.action_affordances.push(Affordance {
        id: id("submit"),
        label: "Submit".to_owned(),
        action_id: submit_action,
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    });
    let mut draft = TypedMap::new();
    draft.insert(id("query"), TypedValue::String("draft query".to_owned()));
    draft.insert(id("removed"), TypedValue::String("stale value".to_owned()));
    let mut expected = TypedMap::new();
    expected.insert(id("query"), TypedValue::String("draft query".to_owned()));
    expected.insert(id("scope"), TypedValue::String("provider scope".to_owned()));

    for action in [ControlAction::Submit, ControlAction::Activate] {
        assert_eq!(
            control_intent(&form, None, None, Some(&draft), action),
            ControlIntent::Event(PanelEvent::Submit {
                values: expected.clone(),
            })
        );
    }
}
