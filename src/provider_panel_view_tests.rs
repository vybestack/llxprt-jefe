use crate::domain::Id;
use crate::runtime::provider::protocol::{
    DetailBody, DetailMetadata, EmptyBody, ErrorBody, FormBody, FormFieldError, ListBody, ListItem,
    PanelBody, ProgressBody, StatusBody, StatusRow, StatusRowState,
};

use super::project_body;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("test id: {error:?}"))
}

#[test]
fn list_projection_preserves_selection_description_status_and_actions() {
    let body = PanelBody::List(ListBody {
        items: vec![ListItem {
            id: id("item-a"),
            label: "Alpha".to_owned(),
            description: Some("first item".to_owned()),
            status: Some("ready".to_owned()),
            actions: vec![id("open")],
        }],
        selected_id: Some(id("item-a")),
        next_page_token: Some("next-page".to_owned()),
    });
    let mut lines = Vec::new();

    project_body(&body, None, &mut lines);

    assert_eq!(
        lines,
        [
            ">> Alpha [ready]".to_owned(),
            "   first item".to_owned(),
            "   actions: open".to_owned(),
            "more results available".to_owned(),
        ]
    );
}

#[test]
fn host_local_list_selection_overrides_the_last_provider_selection() {
    let body = PanelBody::List(ListBody {
        items: vec![
            ListItem {
                id: id("item-a"),
                label: "Alpha".to_owned(),
                description: None,
                status: None,
                actions: Vec::new(),
            },
            ListItem {
                id: id("item-b"),
                label: "Beta".to_owned(),
                description: None,
                status: None,
                actions: Vec::new(),
            },
        ],
        selected_id: Some(id("item-a")),
        next_page_token: None,
    });
    let selected = id("item-b");
    let mut lines = Vec::new();

    project_body(&body, Some(&selected), &mut lines);

    assert_eq!(lines, ["   Alpha".to_owned(), ">> Beta".to_owned()]);
}

#[test]
fn detail_status_progress_empty_and_error_project_all_semantics() {
    let bodies = [
        PanelBody::Detail(DetailBody {
            document: "Document".to_owned(),
            metadata: vec![DetailMetadata {
                label: "Owner".to_owned(),
                value: "Jefe".to_owned(),
            }],
            actions: vec![id("edit")],
        }),
        PanelBody::Status(StatusBody {
            rows: vec![StatusRow {
                label: "Health".to_owned(),
                value: "degraded".to_owned(),
                state: StatusRowState::Warning,
            }],
        }),
        PanelBody::Progress(ProgressBody {
            message: "Loading".to_owned(),
            completed: Some(2),
            total: Some(4),
            cancellable: true,
        }),
        PanelBody::Empty(EmptyBody {
            message: "Nothing here".to_owned(),
            action: Some(id("create")),
        }),
        PanelBody::Error(ErrorBody {
            code: "PLG-E502".to_owned(),
            message: "failed".to_owned(),
            retryable: true,
            retry_action: Some(id("retry")),
        }),
    ];
    let mut lines = Vec::new();
    for body in &bodies {
        project_body(body, None, &mut lines);
    }

    assert_eq!(
        lines,
        [
            "Document".to_owned(),
            "Owner: Jefe".to_owned(),
            "actions: edit".to_owned(),
            "[warning] Health: degraded".to_owned(),
            "Loading 2/4 [Cancel]".to_owned(),
            "Nothing here [create]".to_owned(),
            "PLG-E502 failed [Retry: retry]".to_owned(),
        ]
    );
}

#[test]
fn form_projection_includes_field_errors_and_submit_action() {
    let body = PanelBody::Form(FormBody {
        fields: Vec::new(),
        values: std::collections::BTreeMap::default(),
        field_errors: vec![FormFieldError {
            field_id: id("name"),
            message: "required".to_owned(),
        }],
        submit_action: crate::domain::action_registry::ActionId::parse("submit")
            .unwrap_or_else(|error| panic!("test action id: {error:?}")),
    });
    let mut lines = Vec::new();

    project_body(&body, None, &mut lines);

    assert_eq!(
        lines,
        ["name: required".to_owned(), "submit: submit".to_owned()]
    );
}

#[test]
fn snapshot_affordances_project_enabled_and_disabled_states() {
    let affordances = [
        crate::runtime::provider::protocol::Affordance {
            id: id("open"),
            label: "Open".to_owned(),
            action_id: crate::domain::action_registry::ActionId::parse("open")
                .unwrap_or_else(|error| panic!("action fixture: {error:?}")),
            arguments: None,
            enabled: true,
            unavailable_reason: None,
        },
        crate::runtime::provider::protocol::Affordance {
            id: id("delete"),
            label: "Delete".to_owned(),
            action_id: crate::domain::action_registry::ActionId::parse("delete")
                .unwrap_or_else(|error| panic!("action fixture: {error:?}")),
            arguments: None,
            enabled: false,
            unavailable_reason: Some("read only".to_owned()),
        },
    ];
    let mut lines = Vec::new();

    super::project_affordances(&affordances, &mut lines);

    assert_eq!(
        lines,
        [
            "[open] Open".to_owned(),
            "[delete] Delete (unavailable: read only)".to_owned(),
        ]
    );
}
