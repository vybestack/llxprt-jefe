use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::provider_panel_view::{
    PanelHitTarget, PanelProjection, PanelStatus, ProviderScreenView, project_provider_screen,
};
use crate::runtime::provider::protocol::{
    Affordance, BodyKind, DetailBody, DetailMetadata, EmptyBody, ErrorBody, FormBody,
    FormFieldError, HostLocal, ListBody, ListItem, PanelBody, PanelSnapshot, ProgressBody,
    StatusBody, StatusRow, StatusRowState,
};
use crate::state::provider_panels::{
    AcceptSnapshot, DeclareInput, EventDeclaration, EventKind, MODEL_SCHEMA, PanelInstanceId,
    ProviderPanelState,
};
use crate::workbench::{
    Axis, Insets, LayoutChild, LayoutNode, PanelDescriptor, PanelId, PanelState, PanelTypeId, Rect,
    ResolvedLayout, RouteId, ScreenDescriptor, ScreenId, ScreenIdentity, ScreenInstanceId, Size,
    resolve_layout,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("test id: {error:?}"))
}

fn action_id(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("action id {value:?}: {error:?}"))
}

fn owner() -> Id {
    id("vendor.pkg")
}

fn panel_type() -> Id {
    id("vendor.panel")
}

fn panel_config() -> TypedMap {
    crate::workbench::config::insets_config(Insets::new(2, 1, 1, 1))
        .unwrap_or_else(|| panic!("nonzero insets must produce panel config"))
}

fn weight(value: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(value).unwrap_or_else(|| panic!("test weight must be nonzero"))
}
fn projected_panel<'a>(view: &'a ProviderScreenView, name: &str) -> &'a PanelProjection {
    view.panels
        .iter()
        .find(|panel| panel.id.as_str() == name)
        .unwrap_or_else(|| panic!("projected panel {name:?} must exist"))
}

fn exact_target<'a>(panel: &'a PanelProjection, text: &str) -> Option<&'a PanelHitTarget> {
    panel
        .lines
        .iter()
        .position(|line| line == text)
        .and_then(|index| panel.hit_targets[index].as_ref())
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
    .unwrap_or_else(|error| panic!("form field fixture: {error}"))
}

fn make_descriptor() -> ScreenDescriptor {
    ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Issues),
        title: "Test Screen".to_owned(),
        route: RouteId::from_static("test"),
        panels: vec![
            PanelDescriptor {
                id: PanelId::from_static("main"),
                panel_type: PanelTypeId::from_static("provider-panel"),
                config: panel_config(),
                focusable: true,
                required: true,
                ports: Vec::new(),
            },
            PanelDescriptor {
                id: PanelId::from_static("side"),
                panel_type: PanelTypeId::from_static("provider-panel"),
                config: panel_config(),
                focusable: true,
                required: false,
                ports: Vec::new(),
            },
        ],
        initial_focus: PanelId::from_static("main"),
        focus_order: vec![PanelId::from_static("main"), PanelId::from_static("side")],
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: LayoutNode::Split {
            axis: Axis::Horizontal,
            gap: 0,
            children: vec![
                LayoutChild {
                    node: LayoutNode::Leaf {
                        panel: PanelId::from_static("main"),
                    },
                    size: Size::Weight(weight(3)),
                    min: 0,
                    max: None,
                    collapsible: false,
                    collapse_priority: None,
                },
                LayoutChild {
                    node: LayoutNode::Leaf {
                        panel: PanelId::from_static("side"),
                    },
                    size: Size::Weight(weight(1)),
                    min: 0,
                    max: None,
                    collapsible: true,
                    collapse_priority: Some(0),
                },
            ],
        },
    }
}

fn preview_instance() -> ScreenInstanceId {
    ScreenInstanceId::preview()
}

fn resolve(descriptor: &ScreenDescriptor, cols: u16, rows: u16) -> ResolvedLayout {
    let outer = Rect::new(0, 0, cols, rows);
    resolve_layout(
        descriptor,
        preview_instance(),
        outer,
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| panic!("resolve: {error:?}"))
}

fn project_view(
    descriptor: &ScreenDescriptor,
    state: &ProviderPanelState,
    layout: &ResolvedLayout,
) -> ProviderScreenView {
    project_provider_screen(descriptor, 1, state, layout, &PanelId::from_static("main"))
}

fn affordance(value: &str, label: &str, enabled: bool, reason: Option<&str>) -> Affordance {
    Affordance {
        id: id(value),
        label: label.to_owned(),
        action_id: action_id("vendor.run"),
        arguments: None,
        enabled,
        unavailable_reason: reason.map(ToOwned::to_owned),
    }
}

fn list_item(value: &str, label: &str, description: Option<&str>, actions: &[&str]) -> ListItem {
    ListItem {
        id: id(value),
        label: label.to_owned(),
        description: description.map(ToOwned::to_owned),
        status: None,
        actions: actions.iter().map(|action| id(action)).collect(),
    }
}

fn list_snapshot(
    panel: PanelInstanceId,
    items: Vec<ListItem>,
    selected: &str,
    affordances: Vec<Affordance>,
    next_page_token: Option<&str>,
) -> PanelSnapshot {
    PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation: 1,
        revision: 1,
        kind: BodyKind::List,
        title: "List".to_owned(),
        description: None,
        loading: false,
        action_affordances: affordances,
        body: PanelBody::List(ListBody {
            items,
            selected_id: Some(id(selected)),
            next_page_token: next_page_token.map(ToOwned::to_owned),
        }),
    }
}

fn status_rows(count: usize) -> Vec<StatusRow> {
    (0..count)
        .map(|index| StatusRow {
            label: format!("Row{index}"),
            value: format!("val{index}"),
            state: StatusRowState::Normal,
        })
        .collect()
}

fn declare_and_activate_panel(
    state: &mut ProviderPanelState,
    panel_id: &PanelId,
    screen_instance: u64,
    kinds: &[BodyKind],
) -> PanelInstanceId {
    let events = [
        EventDeclaration {
            kind: EventKind::Selected,
            arguments: Vec::new(),
        },
        EventDeclaration {
            kind: EventKind::Activated,
            arguments: Vec::new(),
        },
        EventDeclaration {
            kind: EventKind::Retry,
            arguments: Vec::new(),
        },
    ];
    let outcome = state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id,
            screen_instance_id: screen_instance,
            panel_type: &panel_type(),
            activation: &TypedMap::new(),
            allowed_model_kinds: kinds,
            allowed_events: &events,
            action_authority: &[action_id("vendor.run")],
            process_generation: 1,
        })
        .unwrap_or_else(|error| panic!("declare: {error:?}"));
    state
        .activate(outcome.instance)
        .unwrap_or_else(|error| panic!("activate: {error:?}"));
    outcome.instance
}

fn accept_snapshot(
    state: &mut ProviderPanelState,
    _panel: PanelInstanceId,
    snapshot: PanelSnapshot,
) {
    state
        .accept_snapshot(AcceptSnapshot {
            owner: &owner(),
            received_process_generation: 1,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot: &snapshot,
        })
        .unwrap_or_else(|error| panic!("accept snapshot: {error:?}"));
}

fn snapshot_with_body(
    panel: PanelInstanceId,
    generation: u64,
    body: PanelBody,
    kind: BodyKind,
) -> PanelSnapshot {
    PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel.as_u64(),
        generation,
        revision: 1,
        kind,
        title: "Panel Title".to_owned(),
        description: None,
        loading: false,
        action_affordances: Vec::new(),
        body,
    }
}

// ---------------------------------------------------------------------------
// Geometry identity tests
// ---------------------------------------------------------------------------

#[test]
fn visible_panels_carry_resolved_chrome_and_content_rects() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let state = ProviderPanelState::new();
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );

    let main = projected_panel(&view, "main");
    let resolved_main = layout
        .panel(&PanelId::from_static("main"))
        .unwrap_or_else(|| panic!("resolved main panel must exist"));
    assert!(main.visible);
    assert_eq!(main.chrome, resolved_main.chrome);
    assert_eq!(main.content, resolved_main.content);
}

#[test]
fn too_small_layout_is_marked() {
    let descriptor = make_descriptor();
    let outer = Rect::new(0, 0, 10, 2);
    let layout = resolve_layout(
        &descriptor,
        preview_instance(),
        outer,
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| panic!("resolve: {error:?}"));
    let state = ProviderPanelState::new();
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    assert!(view.too_small);
}

#[test]
fn focused_panel_is_marked() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let state = ProviderPanelState::new();
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = projected_panel(&view, "main");
    assert!(main.focused);
    let side = projected_panel(&view, "side");
    assert!(!side.focused);
}

// ---------------------------------------------------------------------------
// Lifecycle status tests
// ---------------------------------------------------------------------------

#[test]
fn panel_without_instance_shows_unavailable() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let state = ProviderPanelState::new();
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert_eq!(main.status, PanelStatus::Unavailable);
    assert!(main.lines.contains(&"provider unavailable".to_owned()));
}

#[test]
fn activating_panel_shows_loading() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let _ = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Empty],
    );
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert_eq!(main.status, PanelStatus::Loading);
}

#[test]
fn active_panel_with_snapshot_shows_active() {
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
                message: "nothing".to_owned(),
                action: None,
            }),
            BodyKind::Empty,
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
    assert_eq!(main.status, PanelStatus::Active);
}

// ---------------------------------------------------------------------------
// Body projection tests (all seven kinds)
// ---------------------------------------------------------------------------

#[test]
fn list_body_projects_with_selection_and_pagination() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::List(ListBody {
                items: vec![ListItem {
                    id: id("item-a"),
                    label: "Alpha".to_owned(),
                    description: Some("first".to_owned()),
                    status: Some("ready".to_owned()),
                    actions: vec![id("open")],
                }],
                selected_id: Some(id("item-a")),
                next_page_token: Some("next".to_owned()),
            }),
            BodyKind::List,
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
    assert!(main.lines.iter().any(|l| l.contains(">> Alpha")));
    assert!(main.lines.iter().any(|l| l.contains("first")));
    assert!(main.lines.iter().any(|l| l.contains("actions: open")));
    assert!(main.lines.iter().any(|l| l.contains("more results")));
}

#[test]
fn detail_body_projects_document_metadata_and_actions() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
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
                document: "Doc text".to_owned(),
                metadata: vec![DetailMetadata {
                    label: "Owner".to_owned(),
                    value: "Jefe".to_owned(),
                }],
                actions: vec![id("edit")],
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
    assert!(main.lines.iter().any(|l| l.contains("Doc text")));
    assert!(main.lines.iter().any(|l| l.contains("Owner: Jefe")));
    assert!(main.lines.iter().any(|l| l.contains("actions: edit")));
}

#[test]
fn form_body_projects_fields_errors_and_submit() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Form],
    );
    let field = string_field("vendor.name", "Name");
    let mut values = TypedMap::new();
    values.insert(id("vendor.name"), TypedValue::String("hello".to_owned()));
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Form(FormBody {
                fields: vec![field],
                values,
                field_errors: vec![FormFieldError {
                    field_id: id("vendor.name"),
                    message: "too short".to_owned(),
                }],
                submit_action: action_id("vendor.submit"),
            }),
            BodyKind::Form,
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
    assert!(main.lines.iter().any(|l| l.contains("Name: hello")));
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("vendor.name: too short"))
    );
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("submit: vendor.submit"))
    );
}

#[test]
fn status_body_projects_rows() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Status],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Status(StatusBody {
                rows: vec![StatusRow {
                    label: "Health".to_owned(),
                    value: "degraded".to_owned(),
                    state: StatusRowState::Warning,
                }],
            }),
            BodyKind::Status,
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
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("[warning] Health: degraded"))
    );
}

#[test]
fn progress_body_projects_message_count_and_cancel() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Progress],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Progress(ProgressBody {
                message: "Loading".to_owned(),
                completed: Some(2),
                total: Some(4),
                cancellable: true,
            }),
            BodyKind::Progress,
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
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("Loading 2/4 [Cancel]"))
    );
}

#[test]
fn empty_body_projects_message_and_action() {
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
                message: "Nothing here".to_owned(),
                action: Some(id("create")),
            }),
            BodyKind::Empty,
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
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("Nothing here [create]"))
    );
}

#[test]
fn error_body_projects_code_message_and_retry() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 24);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Error],
    );
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Error(ErrorBody {
                code: "PLG-E502".to_owned(),
                message: "failed".to_owned(),
                retryable: true,
                retry_action: Some(id("retry")),
            }),
            BodyKind::Error,
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
    assert!(
        main.lines
            .iter()
            .any(|l| l.contains("PLG-E502 failed [Retry: retry]"))
    );
}

// ---------------------------------------------------------------------------
// Wrapping and clipping tests
// ---------------------------------------------------------------------------

#[test]
fn long_text_wraps_at_content_width() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 30, 10);
    let content_width = layout
        .panel(&PanelId::from_static("main"))
        .unwrap_or_else(|| panic!("resolved main panel must exist"))
        .content
        .width;
    let long_text = "This is a very long document that should wrap across multiple terminal rows";
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
                document: long_text.to_owned(),
                metadata: Vec::new(),
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
    for line in &main.lines {
        assert!(
            crate::text_wrap::wrap_text(line, content_width as usize).len() <= 1,
            "line '{line}' exceeds content width {content_width}"
        );
    }
    assert!(
        main.lines.len() > 1,
        "long text should wrap into multiple lines"
    );
}

#[test]
fn scroll_offset_clips_body_lines() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 80, 5);
    let content_height = layout
        .panel(&PanelId::from_static("main"))
        .unwrap_or_else(|| panic!("resolved main panel must exist"))
        .content
        .height;
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::Status],
    );
    let rows = status_rows(20);
    accept_snapshot(
        &mut state,
        panel,
        snapshot_with_body(
            panel,
            1,
            PanelBody::Status(StatusBody { rows }),
            BodyKind::Status,
        ),
    );
    state
        .update_host_local(
            panel,
            HostLocal {
                focus_target: None,
                scroll_offset: 5,
                selected_id: None,
                form_draft: None,
            },
        )
        .unwrap_or_else(|error| panic!("host-local fixture: {error}"));
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = &view.panels[0];
    assert!(
        main.lines.len() <= content_height as usize,
        "clipped lines {} should not exceed content height {content_height}",
        main.lines.len()
    );
    assert!(
        main.lines
            .iter()
            .all(|l| !l.contains("Row0") && !l.contains("Row4"))
    );
    assert!(main.lines.iter().any(|l| l.contains("Row5")));
}

#[test]
fn zero_content_height_produces_no_lines() {
    let descriptor = make_descriptor();
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
                message: "msg".to_owned(),
                action: None,
            }),
            BodyKind::Empty,
        ),
    );
    let mut layout = resolve(&descriptor, 80, 24);
    let main_geometry = layout
        .panels
        .iter_mut()
        .find(|resolved| resolved.id == PanelId::from_static("main"))
        .unwrap_or_else(|| panic!("main panel geometry"));
    main_geometry.content.height = 0;
    let view = project_provider_screen(
        &descriptor,
        1,
        &state,
        &layout,
        &PanelId::from_static("main"),
    );
    let main = view
        .panels
        .iter()
        .find(|panel| panel.id == PanelId::from_static("main"))
        .unwrap_or_else(|| panic!("main panel projection"));
    assert!(main.lines.is_empty());
}

// ---------------------------------------------------------------------------
// Stale state
// ---------------------------------------------------------------------------

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
