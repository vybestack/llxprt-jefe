//! Production-path tests for host-owned overlay controls (issue #705 S6).
//!
//! Every projection and semantic input here goes through the sealed
//! HostControl factories — Help/Detail, Search/Confirmation Form, and the
//! provider surface Status/Error bodies — without consulting any provider
//! panel snapshot and without any screen/origin/package identity dispatch.

use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedValue};
use crate::host_controls::{ControlAction, ControlIntent};
use crate::overlay_controls::{
    ConfirmationCommand, ConfirmationContent, HostOverlayLayout, ProviderConfirmationContent,
    confirmation_command, overlay_intent, project_confirmation, project_help,
    project_provider_confirmation, project_provider_surface, provider_surface_footer,
};
use crate::runtime::provider::protocol::{PanelEvent, TypedMap};
use crate::state::ConfirmFocus;
use crate::state::provider_view::{
    ProviderRowStatus, ProviderViewMode, ProviderViewProjection, ProviderViewRow,
};
use crate::state::transition::TransitionExt;
use crate::state::{AppEvent, AppState};

fn help_state() -> AppState {
    AppState::new(crate::test_support::published_workbench())
        .apply(AppEvent::OpenHelp)
        .committed_pure()
}

#[test]
fn production_help_intents_scroll_through_the_detail_factory() {
    let state = help_state();
    let help = project_help(&state, 60);

    assert_eq!(
        overlay_intent(&help, ControlAction::Next),
        ControlIntent::Scroll(1)
    );
    assert_eq!(
        overlay_intent(&help, ControlAction::Previous),
        ControlIntent::Scroll(-1)
    );
}

#[test]
fn one_typed_help_layout_drives_tiny_standard_and_wide_viewports() {
    let state = help_state();
    for (cols, rows, expected) in [
        (3, 1, (3, 1, 0, 0)),
        (8, 6, (8, 6, 4, 0)),
        (60, 24, (60, 24, 56, 18)),
        (120, 40, (60, 40, 56, 34)),
    ] {
        let layout = HostOverlayLayout::help(cols, rows);
        assert_eq!(
            (
                layout.width,
                layout.height,
                layout.content_width,
                layout.viewport_rows,
            ),
            expected
        );
        let projection = project_help(&state, layout.content_width);
        let row_count = projection.text_rows().count();
        assert!(row_count > 0);
        let (delta, max_viewport) = state
            .help_control_scroll(ControlAction::Next, cols, rows)
            .unwrap_or_else(|| panic!("Help Detail projection must accept scrolling"));
        assert_eq!(delta, 1);
        assert_eq!(max_viewport, row_count.saturating_sub(layout.viewport_rows));
    }
}

#[test]
fn typed_help_layout_matches_selection_bounds_at_tiny_standard_and_wide_sizes() {
    for (term_cols, term_rows) in [(3, 1), (8, 6), (60, 24), (120, 40)] {
        let (render_cols, render_rows) = crate::layout::effective_render_size(term_cols, term_rows);
        let expected = HostOverlayLayout::help(render_cols, render_rows);
        let screen_layout = crate::selection::ScreenLayout::new(
            term_cols,
            term_rows,
            crate::workbench::DASHBOARD_IDENTITY,
            false,
            false,
        )
        .with_overlay(crate::selection::OverlayPane::HelpModal);

        let (pane, geometry) = crate::selection::pane_at(0, 0, None, false, &screen_layout)
            .unwrap_or_else(|| panic!("Help origin must be selectable at {term_cols}x{term_rows}"));
        assert_eq!(pane, crate::selection::SelectablePane::HelpModal);
        assert_eq!(geometry.width, expected.width);
        assert_eq!(geometry.height, expected.height);
        assert_eq!(geometry.content_origin_col, 2);
        assert_eq!(geometry.content_origin_row, 2);

        if expected.width < render_cols {
            assert!(
                crate::selection::pane_at(expected.width, 0, None, false, &screen_layout).is_none(),
                "selection outside the rendered Help width must be rejected"
            );
        }
    }
}

#[test]
fn help_copy_rows_are_the_exact_typed_projection_at_every_layout_width() {
    let state = help_state();
    for (cols, rows) in [(3, 1), (8, 6), (60, 24), (120, 40)] {
        let layout = HostOverlayLayout::help(cols, rows);
        let projection = project_help(&state, layout.content_width);
        let content = crate::selection::pane_content_lines(
            crate::selection::SelectablePane::HelpModal,
            &state,
            None,
            &[],
            cols,
            rows,
        );
        let mut expected = vec![projection.title.clone()];
        expected.extend(projection.text_rows().map(str::to_owned));
        assert_eq!(content.lines, expected, "Help copy drift at {cols}x{rows}");
    }
}
#[test]
fn production_search_submit_flows_through_the_form_factory() {
    let state = AppState::new(crate::test_support::published_workbench())
        .apply(AppEvent::OpenSearch)
        .committed_pure()
        .apply(AppEvent::FormChar('p'))
        .committed_pure()
        .apply(AppEvent::FormChar('r'))
        .committed_pure();
    let search = crate::overlay_controls::project_search(&state, 60);

    let ControlIntent::Event(PanelEvent::Submit { values }) =
        overlay_intent(&search, ControlAction::Activate)
    else {
        unreachable!("the form factory must submit the live query");
    };
    let Ok(query_id) = crate::domain::Id::parse("query") else {
        unreachable!("the query field id is canonical");
    };
    assert_eq!(
        values.get(&query_id).cloned(),
        Some(crate::domain::TypedValue::String("pr".to_owned()))
    );
}

#[test]
fn production_confirmation_submit_reflects_the_focused_decision() {
    let cancel = project_confirmation(
        ConfirmationContent {
            title: "Confirm",
            message: "Proceed?",
            show_delete_work_dir: false,
            delete_work_dir: false,
            focus: ConfirmFocus::Cancel,
        },
        60,
    );
    let confirm = project_confirmation(
        ConfirmationContent {
            title: "Confirm",
            message: "Proceed?",
            show_delete_work_dir: false,
            delete_work_dir: false,
            focus: ConfirmFocus::Confirm,
        },
        60,
    );

    assert_eq!(
        overlay_intent(&cancel, ControlAction::Next),
        ControlIntent::Scroll(1)
    );
    for (projection, expected) in [(&cancel, "Cancel"), (&confirm, "Confirm")] {
        let ControlIntent::Event(PanelEvent::Submit { values }) =
            overlay_intent(projection, ControlAction::Activate)
        else {
            unreachable!("the form factory must submit the focused decision");
        };
        let Ok(decision_id) = crate::domain::Id::parse("decision") else {
            unreachable!("the decision field id is canonical");
        };
        assert_eq!(
            values.get(&decision_id).cloned(),
            Some(crate::domain::TypedValue::String(expected.to_owned()))
        );
    }
}

#[test]
fn confirmation_prompt_and_decision_are_distinct_factory_rows() {
    let projection = project_confirmation(
        ConfirmationContent {
            title: "Confirm",
            message: "Proceed?",
            show_delete_work_dir: false,
            delete_work_dir: false,
            focus: ConfirmFocus::Cancel,
        },
        60,
    );
    let rows = projection
        .rows
        .iter()
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>();

    assert!(rows.contains(&"Proceed?"));
    assert!(rows.contains(&"Decision: Cancel"));
    assert!(!rows.contains(&"Proceed?: Cancel"));
}

#[test]
fn confirmation_commands_derive_from_the_factory_intents() {
    let cancel = project_confirmation(
        ConfirmationContent {
            title: "Confirm",
            message: "Proceed?",
            show_delete_work_dir: false,
            delete_work_dir: false,
            focus: ConfirmFocus::Cancel,
        },
        60,
    );
    let confirm = project_confirmation(
        ConfirmationContent {
            title: "Confirm",
            message: "Proceed?",
            show_delete_work_dir: false,
            delete_work_dir: false,
            focus: ConfirmFocus::Confirm,
        },
        60,
    );

    assert_eq!(
        confirmation_command(&cancel, ControlAction::Next),
        Some(ConfirmationCommand::CycleFocus)
    );
    assert_eq!(
        confirmation_command(&cancel, ControlAction::Activate),
        Some(ConfirmationCommand::ChooseCancel)
    );
    assert_eq!(
        confirmation_command(&confirm, ControlAction::Activate),
        Some(ConfirmationCommand::ChooseConfirm)
    );
    assert_eq!(
        confirmation_command(&cancel, ControlAction::Retry),
        None,
        "a control action the factory does not interpret must command nothing"
    );
}

#[test]
fn provider_confirmation_projects_the_declared_decision_without_a_provider_snapshot() {
    let values = TypedMap::new();
    let confirmation = project_provider_confirmation(
        ProviderConfirmationContent {
            title: "Confirm deployment?",
            body: "This action changes production.",
            confirm_label: "Deploy now",
            focus: ConfirmFocus::Confirm,
            continuation_schema: &[],
            continuation_values: &values,
            focused_field: None,
        },
        64,
    );

    assert_eq!(confirmation.kind, crate::host_controls::ControlKind::Form);
    assert_eq!(confirmation.title, "Confirm deployment?");
    assert!(
        confirmation
            .rows
            .iter()
            .any(|row| row.text.contains("Deploy now")),
        "the focused decision must render its declared label"
    );
    assert_eq!(
        confirmation_command(&confirmation, ControlAction::Activate),
        Some(ConfirmationCommand::ChooseConfirm)
    );

    let cancelled = project_provider_confirmation(
        ProviderConfirmationContent {
            title: "Confirm deployment?",
            body: "This action changes production.",
            confirm_label: "Deploy now",
            focus: ConfirmFocus::Cancel,
            continuation_schema: &[],
            continuation_values: &values,
            focused_field: None,
        },
        64,
    );
    assert_eq!(
        confirmation_command(&cancelled, ControlAction::Activate),
        Some(ConfirmationCommand::ChooseCancel)
    );
}

fn provider_row(label: &str, status: ProviderRowStatus, focused: bool) -> ProviderViewRow {
    ProviderViewRow {
        label: label.to_owned(),
        status,
        focused,
    }
}

fn continuation_field(id: &str, label: &str) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|error| panic!("field id: {error}")),
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
    .unwrap_or_else(|error| panic!("field: {error}"))
}

#[test]
fn provider_confirmation_projects_instance_owned_fields_and_exact_typed_values() {
    let field = continuation_field("release-channel", "Release channel");
    let mut continuation_values = crate::domain::TypedMap::new();
    continuation_values.insert(field.id().clone(), TypedValue::String("stable".to_owned()));
    let projection = ProviderViewProjection {
        mode: ProviderViewMode::Confirmation {
            confirm_focus: ConfirmFocus::Cancel,
            title: "Confirm release?".to_owned(),
            body: "Publish the selected release.".to_owned(),
            confirm_label: "Publish".to_owned(),
            continuation_schema: vec![field.clone()],
            continuation_values,
            focused_field: Some(field.id().clone()),
        },
        rows: Vec::new(),
        has_active_request: false,
    };

    let confirmation = project_provider_surface(&projection, 64);

    assert!(
        confirmation
            .rows
            .iter()
            .any(|row| row.text.contains("Release channel: stable")),
        "provider fields must render through the shared Form projection: {:?}",
        confirmation.rows
    );
    assert!(
        confirmation
            .rows
            .iter()
            .any(|row| row.text.contains("Publish")),
        "the host decision must share the same Form projection: {:?}",
        confirmation.rows
    );
}

#[test]
fn active_provider_surface_projects_through_the_progress_factory() {
    let projection = ProviderViewProjection {
        mode: ProviderViewMode::Focused,
        rows: vec![provider_row(
            "Ship release",
            ProviderRowStatus::InProgress("Uploading release: 1 / 2".to_owned()),
            true,
        )],
        has_active_request: true,
    };

    let progress = project_provider_surface(&projection, 64);
    assert_eq!(progress.kind, crate::host_controls::ControlKind::Progress);
    assert_eq!(progress.title, "Provider Action");
    assert_eq!(
        progress.rows[0].text,
        "Ship release  Uploading release: 1 / 2 [Cancel]"
    );
    assert_eq!(provider_surface_footer(&projection), "Esc Cancel");
}

#[test]
fn provider_surface_footer_matches_terminal_unavailable_and_confirmation_states() {
    let terminal = ProviderViewProjection {
        mode: ProviderViewMode::Normal,
        rows: vec![provider_row(
            "Ship release",
            ProviderRowStatus::Completed("Release shipped".to_owned()),
            false,
        )],
        has_active_request: false,
    };
    assert_eq!(
        provider_surface_footer(&terminal),
        "Enter Retry   Esc Close"
    );

    let unavailable = ProviderViewProjection {
        mode: ProviderViewMode::Unavailable {
            reason: "provider stopped".to_owned(),
        },
        rows: Vec::new(),
        has_active_request: false,
    };
    let unavailable_projection = project_provider_surface(&unavailable, 64);
    assert!(
        unavailable_projection
            .rows
            .iter()
            .any(|row| row.text.contains("provider stopped"))
    );
    assert_eq!(provider_surface_footer(&unavailable), "Esc Close");

    let confirming = ProviderViewProjection {
        mode: ProviderViewMode::Confirmation {
            confirm_focus: ConfirmFocus::Cancel,
            title: "Confirm deployment?".to_owned(),
            body: "This action changes production.".to_owned(),
            confirm_label: "Deploy now".to_owned(),
            continuation_schema: Vec::new(),
            continuation_values: crate::domain::TypedMap::new(),
            focused_field: None,
        },
        rows: Vec::new(),
        has_active_request: false,
    };
    assert_eq!(
        project_provider_surface(&confirming, 64).kind,
        crate::host_controls::ControlKind::Form
    );
    assert_eq!(
        provider_surface_footer(&confirming),
        "Tab Select   Enter Activate   Esc Cancel"
    );
}

#[test]
fn provider_surface_error_projects_through_the_error_factory() {
    let projection = ProviderViewProjection {
        mode: ProviderViewMode::Error {
            message: "provider exited".to_owned(),
        },
        rows: Vec::new(),
        has_active_request: false,
    };
    let error = project_provider_surface(&projection, 64);
    assert_eq!(error.kind, crate::host_controls::ControlKind::Error);
    assert!(
        error
            .rows
            .iter()
            .any(|row| row.text.contains("provider exited"))
    );
}

#[test]
fn provider_surface_small_viewport_keeps_a_stable_factory_projection() {
    let projection = ProviderViewProjection {
        mode: ProviderViewMode::Small,
        rows: vec![provider_row(
            "Ship release",
            ProviderRowStatus::Unavailable("provider stopped".to_owned()),
            false,
        )],
        has_active_request: false,
    };
    let small = project_provider_surface(&projection, 8);
    assert!(
        small
            .rows
            .iter()
            .any(|row| row.text.contains("Ship release")),
        "small viewports still render the same factory rows"
    );
    assert_eq!(provider_surface_footer(&projection), "Esc Close");
}
