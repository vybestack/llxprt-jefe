//! Semantic panel-event validation tables (issue #391).
//!
//! Every event kind is exercised for acceptance and rejection: valid events
//! emit exactly one [`PanelEventEffect`], while undeclared, invalid, disabled,
//! or stale events emit zero effects and perform zero mutation.

use super::{
    AcceptSnapshot, DeclareInput, EventDeclaration, EventKind, EventOutcome, MODEL_SCHEMA,
    PanelError, PanelInstanceId, PanelLifecycle, ProviderPanelState, SubmitEvent,
};
use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{
    Affordance, BodyKind, DetailBody, EmptyBody, ErrorBody, FormBody, ListBody, ListItem,
    PanelBody, PanelEvent, PanelSnapshot, ProgressBody, StructuredDiffBody, StructuredDiffFile,
    TreeBody, TreeNode,
};
use crate::test_support::{Must, MustErr};
use crate::workbench::PanelId;

fn id(text: &str) -> Id {
    Id::parse(text).unwrap_or_else(|error| panic!("valid id {text:?}: {error:?}"))
}

fn owner() -> Id {
    id("vendor.pkg")
}

fn panel_type() -> Id {
    id("vendor.panel")
}

fn action_id(text: &str) -> ActionId {
    ActionId::parse(text).unwrap_or_else(|error| panic!("valid action id {text:?}: {error:?}"))
}

fn declare_and_activate(state: &mut ProviderPanelState) -> PanelInstanceId {
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
        EventDeclaration {
            kind: EventKind::Retry,
            arguments: Vec::new(),
        },
        EventDeclaration {
            kind: EventKind::Cancel,
            arguments: Vec::new(),
        },
    ];
    let action_authority = [
        action_id("vendor.run"),
        action_id("vendor.submit"),
        action_id("vendor.open"),
    ];
    let outcome = state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id: &PanelId::from_static("main"),
            screen_instance_id: 2,
            panel_type: &panel_type(),
            activation: &TypedMap::new(),
            allowed_model_kinds: &[
                BodyKind::List,
                BodyKind::Tree,
                BodyKind::Detail,
                BodyKind::StructuredDiff,
                BodyKind::Form,
                BodyKind::Status,
                BodyKind::Progress,
                BodyKind::Empty,
                BodyKind::Error,
            ],
            allowed_events: &allowed_events,
            action_authority: &action_authority,
            process_generation: 1,
        })
        .must("declare");
    state.activate(outcome.instance).must("activate");
    outcome.instance
}

fn accept(state: &mut ProviderPanelState, _panel: PanelInstanceId, snapshot: &PanelSnapshot) {
    state
        .accept_snapshot(AcceptSnapshot {
            owner: &owner(),
            received_process_generation: 1,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot,
        })
        .must("snapshot accepted");
}

fn submit(
    state: &mut ProviderPanelState,
    panel: PanelInstanceId,
    generation: u64,
    revision: u64,
    event: PanelEvent,
    allowed: &[EventDeclaration],
) -> Result<EventOutcome, PanelError> {
    state.submit_event(SubmitEvent {
        panel,
        owner: &owner(),
        received_process_generation: 1,
        generation,
        revision,
        event,
        allowed_events: allowed,
    })
}

fn list_panel(
    state: &mut ProviderPanelState,
    item_ids: &[&str],
    next_page_token: Option<&str>,
) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let items: Vec<ListItem> = item_ids
        .iter()
        .map(|item_id| ListItem {
            id: id(item_id),
            label: (*item_id).to_string(),
            description: None,
            status: None,
            actions: vec![],
        })
        .collect();
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::List,
        title: "list".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::List(ListBody {
            items,
            selected_id: None,
            next_page_token: next_page_token.map(str::to_string),
        }),
    };
    accept(state, panel, &snapshot);
    (panel, 1)
}

fn tree_panel(state: &mut ProviderPanelState) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Tree,
        title: "tree".to_owned(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::Tree(TreeBody {
            schema_version: 1,
            nodes: vec![TreeNode {
                id: id("vendor.node"),
                parent_id: None,
                label: "Node".to_owned(),
                semantic_key: id("node"),
                depth: 0,
                expandable: true,
                expanded: false,
            }],
            selected_id: None,
        }),
    };
    accept(state, panel, &snapshot);
    (panel, 1)
}

fn structured_diff_panel(state: &mut ProviderPanelState) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::StructuredDiff,
        title: "diff".to_owned(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::StructuredDiff(StructuredDiffBody {
            schema_version: 1,
            files: vec![StructuredDiffFile {
                id: id("vendor.file"),
                old_path: Some("a/file".to_owned()),
                new_path: Some("b/file".to_owned()),
                old_mode: None,
                new_mode: None,
                binary: true,
                hunks: vec![],
            }],
            selected_file_id: None,
        }),
    };
    accept(state, panel, &snapshot);
    (panel, 1)
}

fn assert_emitted(outcome: Result<EventOutcome, PanelError>) -> PanelEvent {
    match outcome.must("event processed") {
        EventOutcome::Event(effect) => effect.event,
        other => panic!("expected EventOutcome::Event, got {other:?}"),
    }
}

fn assert_zero_effect(outcome: Result<EventOutcome, PanelError>) {
    match outcome.must("event processed") {
        EventOutcome::None => {}
        other => panic!("expected zero effect, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Selected / Activated
// ---------------------------------------------------------------------------

#[test]
fn selected_existing_item_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a", "vendor.b"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    let event = assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Selected { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        }],
    ));
    assert!(matches!(event, PanelEvent::Selected { .. }));
}

#[test]
fn selected_unknown_item_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Selected {
            id: id("vendor.missing"),
        },
        &[EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        }],
    ));
}

#[test]
fn activated_existing_item_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Activated { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Activated,
            arguments: vec![],
        }],
    ));
}

#[test]
fn tree_nodes_and_diff_files_are_valid_selection_targets() {
    let allowed = [
        EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        },
        EventDeclaration {
            kind: EventKind::Activated,
            arguments: vec![],
        },
    ];

    let mut tree_state = ProviderPanelState::new();
    let (tree_panel, tree_generation) = tree_panel(&mut tree_state);
    let tree_revision = tree_state
        .accepted_revision(tree_panel)
        .must("tree revision");
    assert_emitted(submit(
        &mut tree_state,
        tree_panel,
        tree_generation,
        tree_revision,
        PanelEvent::Selected {
            id: id("vendor.node"),
        },
        &allowed,
    ));

    let mut diff_state = ProviderPanelState::new();
    let (diff_panel, diff_generation) = structured_diff_panel(&mut diff_state);
    let diff_revision = diff_state
        .accepted_revision(diff_panel)
        .must("diff revision");
    assert_emitted(submit(
        &mut diff_state,
        diff_panel,
        diff_generation,
        diff_revision,
        PanelEvent::Activated {
            id: id("vendor.file"),
        },
        &allowed,
    ));
}

// ---------------------------------------------------------------------------
// ExpansionChanged
// ---------------------------------------------------------------------------

#[test]
fn expansion_changed_requires_an_expandable_tree_node_and_a_state_change() {
    let allowed = [EventDeclaration {
        kind: EventKind::ExpansionChanged,
        arguments: vec![],
    }];

    let mut state = ProviderPanelState::new();
    let (panel, generation) = tree_panel(&mut state);
    let revision = state.accepted_revision(panel).must("expected value");
    let event = assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::ExpansionChanged {
            id: id("vendor.node"),
            expanded: true,
        },
        &allowed,
    ));
    assert!(matches!(
        event,
        PanelEvent::ExpansionChanged { expanded: true, .. }
    ));

    for event in [
        PanelEvent::ExpansionChanged {
            id: id("vendor.node"),
            expanded: false,
        },
        PanelEvent::ExpansionChanged {
            id: id("vendor.unknown"),
            expanded: true,
        },
    ] {
        assert_zero_effect(submit(
            &mut state,
            panel,
            generation,
            revision,
            event,
            &allowed,
        ));
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

#[test]
fn action_on_enabled_affordance_emits_event() {
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate(&mut state);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Empty,
        title: "t".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![Affordance {
            id: id("vendor.act"),
            label: "Act".to_string(),
            action_id: action_id("vendor.run"),
            arguments: None,
            enabled: true,
            unavailable_reason: None,
        }],
        body: PanelBody::Empty(EmptyBody {
            message: String::new(),
            action: None,
        }),
    };
    accept(&mut state, panel, &snapshot);
    let revision = state.accepted_revision(panel).must("expected value");
    let event = assert_emitted(submit(
        &mut state,
        panel,
        1,
        revision,
        PanelEvent::Action {
            id: id("vendor.act"),
            arguments: TypedMap::new(),
        },
        &[EventDeclaration {
            kind: EventKind::Action,
            arguments: vec![],
        }],
    ));
    assert!(matches!(event, PanelEvent::Action { .. }));
}

#[test]
fn action_on_disabled_affordance_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate(&mut state);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Empty,
        title: "t".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![Affordance {
            id: id("vendor.act"),
            label: "Act".to_string(),
            action_id: action_id("vendor.run"),
            arguments: None,
            enabled: false,
            unavailable_reason: Some("nope".to_string()),
        }],
        body: PanelBody::Empty(EmptyBody {
            message: String::new(),
            action: None,
        }),
    };
    accept(&mut state, panel, &snapshot);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        1,
        revision,
        PanelEvent::Action {
            id: id("vendor.act"),
            arguments: TypedMap::new(),
        },
        &[EventDeclaration {
            kind: EventKind::Action,
            arguments: vec![],
        }],
    ));
}

#[test]
fn action_with_undeclared_id_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Action {
            id: id("vendor.missing"),
            arguments: TypedMap::new(),
        },
        &[EventDeclaration {
            kind: EventKind::Action,
            arguments: vec![],
        }],
    ));
}

// ---------------------------------------------------------------------------
// FieldChanged / Submit
// ---------------------------------------------------------------------------

fn form_field(field_id: &str) -> Field {
    Field::parse(FieldDraft {
        id: id(field_id),
        label: field_id.to_string(),
        description: None,
        kind: FieldKind::String,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: vec![],
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .must("valid field")
}

fn form_panel(
    state: &mut ProviderPanelState,
    field_id: &str,
    submit_action: &str,
) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let field = form_field(field_id);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Form,
        title: "form".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![Affordance {
            id: id(submit_action),
            label: "Submit".to_string(),
            action_id: action_id(submit_action),
            arguments: None,
            enabled: true,
            unavailable_reason: None,
        }],
        body: PanelBody::Form(FormBody {
            fields: vec![field],
            values: TypedMap::new(),
            field_errors: vec![],
            submit_action: action_id(submit_action),
        }),
    };
    accept(state, panel, &snapshot);
    (panel, 1)
}

#[test]
fn field_changed_existing_field_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = form_panel(&mut state, "vendor.f", "vendor.submit");
    let revision = state.accepted_revision(panel).must("expected value");
    let event = assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::FieldChanged {
            field_id: id("vendor.f"),
            value: TypedValue::String("v".to_string()),
        },
        &[EventDeclaration {
            kind: EventKind::FieldChanged,
            arguments: vec![],
        }],
    ));
    assert!(matches!(event, PanelEvent::FieldChanged { .. }));
}

#[test]
fn field_changed_unknown_field_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = form_panel(&mut state, "vendor.f", "vendor.submit");
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::FieldChanged {
            field_id: id("vendor.missing"),
            value: TypedValue::String("v".to_string()),
        },
        &[EventDeclaration {
            kind: EventKind::FieldChanged,
            arguments: vec![],
        }],
    ));
}

#[test]
fn submit_with_known_values_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = form_panel(&mut state, "vendor.f", "vendor.submit");
    let revision = state.accepted_revision(panel).must("expected value");
    let mut values = TypedMap::new();
    values.insert(id("vendor.f"), TypedValue::String("v".to_string()));
    assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Submit { values },
        &[EventDeclaration {
            kind: EventKind::Submit,
            arguments: vec![form_field("vendor.f")],
        }],
    ));
}

#[test]
fn submit_with_unknown_field_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = form_panel(&mut state, "vendor.f", "vendor.submit");
    let revision = state.accepted_revision(panel).must("expected value");
    let mut values = TypedMap::new();
    values.insert(id("vendor.missing"), TypedValue::String("v".to_string()));
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Submit { values },
        &[EventDeclaration {
            kind: EventKind::Submit,
            arguments: vec![],
        }],
    ));
}

// ---------------------------------------------------------------------------
// PageRequested
// ---------------------------------------------------------------------------

#[test]
fn page_requested_with_matching_token_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], Some("vendor.next"));
    let revision = state.accepted_revision(panel).must("expected value");
    assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::PageRequested {
            token: "vendor.next".to_string(),
        },
        &[EventDeclaration {
            kind: EventKind::PageRequested,
            arguments: vec![],
        }],
    ));
}

#[test]
fn page_requested_with_wrong_token_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], Some("vendor.next"));
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::PageRequested {
            token: "vendor.wrong".to_string(),
        },
        &[EventDeclaration {
            kind: EventKind::PageRequested,
            arguments: vec![],
        }],
    ));
}

#[test]
fn page_requested_without_next_token_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::PageRequested {
            token: "vendor.any".to_string(),
        },
        &[EventDeclaration {
            kind: EventKind::PageRequested,
            arguments: vec![],
        }],
    ));
}

// ---------------------------------------------------------------------------
// Cancel
// ---------------------------------------------------------------------------

fn progress_panel(state: &mut ProviderPanelState, cancellable: bool) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Progress,
        title: "progress".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::Progress(ProgressBody {
            message: String::new(),
            completed: None,
            total: None,
            cancellable,
        }),
    };
    accept(state, panel, &snapshot);
    (panel, 1)
}

#[test]
fn cancel_when_cancellable_emits_event() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = progress_panel(&mut state, true);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_emitted(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Cancel,
        &[EventDeclaration {
            kind: EventKind::Cancel,
            arguments: vec![],
        }],
    ));
}

#[test]
fn cancel_when_not_cancellable_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = progress_panel(&mut state, false);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Cancel,
        &[EventDeclaration {
            kind: EventKind::Cancel,
            arguments: vec![],
        }],
    ));
}

// ---------------------------------------------------------------------------
// LinkSelected
// ---------------------------------------------------------------------------

fn detail_panel_with_link(state: &mut ProviderPanelState) -> (PanelInstanceId, u64) {
    let panel = declare_and_activate(state);
    let link_id = id("vendor.link");
    let snapshot = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::Detail,
        title: "detail".to_owned(),
        description: None,
        loading: false,
        action_affordances: vec![Affordance {
            id: link_id.clone(),
            label: "Link".to_owned(),
            action_id: action_id("vendor.open"),
            arguments: None,
            enabled: true,
            unavailable_reason: None,
        }],
        body: PanelBody::Detail(DetailBody {
            document: "document".to_owned(),
            metadata: Vec::new(),
            actions: vec![link_id],
        }),
    };
    accept(state, panel, &snapshot);
    (panel, snapshot.generation)
}

#[test]
fn link_selected_requires_an_enabled_detail_action() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = detail_panel_with_link(&mut state);
    let revision = state.accepted_revision(panel).must("expected value");
    let declaration = [EventDeclaration {
        kind: EventKind::LinkSelected,
        arguments: vec![],
    }];

    assert!(matches!(
        assert_emitted(submit(
            &mut state,
            panel,
            generation,
            revision,
            PanelEvent::LinkSelected {
                link_id: id("vendor.link"),
            },
            &declaration,
        )),
        PanelEvent::LinkSelected { .. }
    ));
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::LinkSelected {
            link_id: id("vendor.unknown"),
        },
        &declaration,
    ));
}

#[test]
fn active_error_retry_requires_retryable_body_and_enabled_affordance() {
    for (retryable, enabled, emits) in [
        (true, true, true),
        (false, true, false),
        (true, false, false),
    ] {
        let mut state = ProviderPanelState::new();
        let panel = declare_and_activate(&mut state);
        let retry = id("retry");
        accept(
            &mut state,
            panel,
            &PanelSnapshot {
                model_schema: MODEL_SCHEMA,
                panel_instance_id: panel.as_u64(),
                generation: 1,
                revision: 1,
                kind: BodyKind::Error,
                title: "error".to_owned(),
                description: None,
                loading: false,
                action_affordances: vec![Affordance {
                    id: retry.clone(),
                    label: "Retry".to_owned(),
                    action_id: action_id("vendor.run"),
                    arguments: None,
                    enabled,
                    unavailable_reason: (!enabled).then(|| "not now".to_owned()),
                }],
                body: PanelBody::Error(ErrorBody {
                    code: "failed".to_owned(),
                    message: "try again".to_owned(),
                    retryable,
                    retry_action: Some(retry),
                }),
            },
        );

        let outcome = state
            .submit_live_event(panel, PanelEvent::Retry)
            .must("retry event processed");
        assert_eq!(matches!(outcome, EventOutcome::Event(_)), emits);
    }
}

// ---------------------------------------------------------------------------
// Undeclared / stale / non-active events emit zero effect
// ---------------------------------------------------------------------------

#[test]
fn undeclared_event_kind_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        revision,
        PanelEvent::Selected { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Cancel,
            arguments: vec![],
        }],
    ));
}

#[test]
fn stale_generation_event_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    let revision = state.accepted_revision(panel).must("expected value");
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation + 99,
        revision,
        PanelEvent::Selected { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        }],
    ));
}

#[test]
fn stale_revision_event_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let (panel, generation) = list_panel(&mut state, &["vendor.a"], None);
    assert_zero_effect(submit(
        &mut state,
        panel,
        generation,
        999,
        PanelEvent::Selected { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        }],
    ));
}

#[test]
fn event_on_non_active_panel_emits_zero_effect() {
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate(&mut state);
    // Still Activating (no accepted snapshot): a Selected event cannot reference
    // an accepted model and must emit zero effect.
    assert_zero_effect(submit(
        &mut state,
        panel,
        1,
        1,
        PanelEvent::Selected { id: id("vendor.a") },
        &[EventDeclaration {
            kind: EventKind::Selected,
            arguments: vec![],
        }],
    ));
    assert_eq!(state.lifecycle(panel), Some(PanelLifecycle::Activating));
}

#[test]
fn event_to_unknown_panel_is_an_error() {
    let mut state = ProviderPanelState::new();
    let missing = PanelInstanceId::from_u64(9999);
    let error = submit(
        &mut state,
        missing,
        1,
        1,
        PanelEvent::Cancel,
        &[EventDeclaration {
            kind: EventKind::Cancel,
            arguments: vec![],
        }],
    )
    .must_err("expected failure");
    assert!(matches!(error, PanelError::UnknownPanel));
}

#[test]
fn live_event_uses_the_schema_retained_at_declaration() {
    let mut state = ProviderPanelState::default();
    let (panel, _) = list_panel(&mut state, &["known"], None);

    assert!(matches!(
        state.submit_live_event(panel, PanelEvent::Selected { id: id("known") },),
        Ok(EventOutcome::Event(_))
    ));
    assert!(matches!(
        state.submit_live_event(
            panel,
            PanelEvent::Action {
                id: id("undeclared"),
                arguments: TypedMap::new(),
            },
        ),
        Ok(EventOutcome::None)
    ));
}
