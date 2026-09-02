//! The GitHub-backed workspace screen constructors (issue #384, CW04-01).
//!
//! The issues, pull-request, and actions screens differ only in identity and
//! their two content panels, so they are described once by [`WorkspaceSpec`]
//! and built by one shared constructor rather than duplicated. This module is
//! the `screens` split for the 1000-line source-size gate: the constructors
//! moved here verbatim, and `screens` keeps the registry and every other
//! screen.

use super::descriptor::{
    LayoutNode, PanelDescriptor, PortDescriptor, PortDirection, ScreenDescriptor,
};
use super::ids::{IdError, PanelId, RouteId, ScreenId, ScreenIdentity};
use super::panel_types::FILTER_BAND_PANEL_TYPE;
use super::screens::{
    ACTIONS_LIST_PANEL, BAND_CHROME, BANNER_ROWS, BORDERED_BAND_CHROME, DETAIL_MIN_ROWS,
    DETAIL_PANE_CHROME, DETAIL_WEIGHT, FILTER_CONTROLS_ROWS, FLEX_MIN_COLUMNS, HOST_OVERLAYS,
    ISSUES_LIST_PANEL, LIST_MIN_ROWS, LIST_PANE_CHROME, LIST_WEIGHT, PULL_REQUESTS_LIST_PANEL,
    REPOSITORIES_PANEL, RegistryError, SIDEBAR_COLUMNS, band_child, collapsible_child, column,
    fixed_child, focus_order, leaf, panel, required_child, row, sidebar_panel, weight,
};
use super::screens_ports::{selection_port, subject_port, workspace_relationships};

/// The workspace column shared by the issues, pull-request, and actions
/// screens: an optional banner, an optional filter band, then a list over a
/// detail pane in a three-to-seven split.
fn workspace_column(
    banner: &'static str,
    filter: &'static str,
    list: &'static str,
    detail: &'static str,
) -> Result<LayoutNode, IdError> {
    Ok(column(vec![
        band_child(leaf(banner)?, BANNER_ROWS, -100),
        band_child(leaf(filter)?, FILTER_CONTROLS_ROWS, -99),
        required_child(leaf(list)?, weight(LIST_WEIGHT), LIST_MIN_ROWS),
        collapsible_child(leaf(detail)?, weight(DETAIL_WEIGHT), DETAIL_MIN_ROWS, 0),
    ]))
}

/// The values that distinguish one workspace screen from another.
///
/// The three GitHub-backed screens differ only in their identity and their two
/// content panels, so they are described rather than duplicated.
struct WorkspaceSpec {
    /// Stable screen identity.
    id: ScreenId,
    /// Screen title.
    title: &'static str,
    /// Navigation route.
    route: &'static str,
    /// Identity of the list panel; also its panel type.
    list: &'static str,
    /// Identity of the detail panel; also its panel type.
    detail: &'static str,
    /// Identity of the conditional notice banner.
    banner: &'static str,
    /// Identity of the conditional filter-controls band.
    filter: &'static str,
    /// Immutable owner of the subject resource schema.
    subject_owner: Option<&'static str>,
    /// Versioned type the list publishes and the detail consumes, when the
    /// screen couples them.
    ///
    /// `None` means the screen's detail pane is not driven by its list
    /// selection, so it declares no ports and no relationship.
    subject_type: Option<&'static str>,
}

/// Attach an optional port to a panel.
fn ported(mut panel: PanelDescriptor, port: Option<PortDescriptor>) -> PanelDescriptor {
    panel.ports.extend(port);
    panel
}

/// A workspace screen: the repository sidebar beside the shared column.
fn workspace_screen(spec: &WorkspaceSpec) -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(spec.id),
        title: spec.title.to_owned(),
        route: RouteId::parse(spec.route)?,
        panels: vec![
            sidebar_panel()?,
            panel(spec.banner, "notice-band", false, false, BAND_CHROME)?,
            panel(
                spec.filter,
                FILTER_BAND_PANEL_TYPE,
                false,
                false,
                BORDERED_BAND_CHROME,
            )?,
            ported(
                panel(spec.list, spec.list, true, true, LIST_PANE_CHROME)?,
                selection_port(spec.subject_owner, spec.subject_type, PortDirection::Output)?,
            ),
            ported(
                panel(spec.detail, spec.detail, true, false, DETAIL_PANE_CHROME)?,
                subject_port(spec.subject_owner, spec.subject_type, PortDirection::Input)?,
            ),
        ],
        initial_focus: PanelId::parse(spec.list)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, spec.list, spec.detail])?,
        relationships: workspace_relationships(spec.list, spec.detail, spec.subject_type)?,
        activation: Vec::new(),
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: Vec::new(),
        bindings: Vec::new(),
        layout: row(vec![
            fixed_child(leaf(REPOSITORIES_PANEL)?, SIDEBAR_COLUMNS),
            required_child(
                workspace_column(spec.banner, spec.filter, spec.list, spec.detail)?,
                weight(1),
                FLEX_MIN_COLUMNS,
            ),
        ]),
    })
}

/// `github.issues` — issue list over issue detail.
pub(super) fn issues_screen() -> Result<ScreenDescriptor, RegistryError> {
    workspace_screen(&WorkspaceSpec {
        id: ScreenId::Issues,
        title: "Issues",
        route: "issues",
        list: ISSUES_LIST_PANEL,
        detail: "issue-detail",
        banner: "issue-list-banner",
        filter: "issue-list-filter",
        subject_owner: Some("github.issues"),
        subject_type: Some("github.issue@1"),
    })
}

/// `github.pull-requests` — PR list over PR detail, which also hosts the
/// review threads, actions, and merge affordances.
pub(super) fn pull_requests_screen() -> Result<ScreenDescriptor, RegistryError> {
    workspace_screen(&WorkspaceSpec {
        id: ScreenId::PullRequests,
        title: "Pull Requests",
        route: "pull-requests",
        list: PULL_REQUESTS_LIST_PANEL,
        detail: "pr-detail",
        banner: "pr-list-banner",
        filter: "pr-list-filter",
        subject_owner: Some("github.pull-requests"),
        subject_type: Some("github.pull-request@1"),
    })
}

/// `github.actions` — workflow-run list over run detail.
pub(super) fn actions_screen() -> Result<ScreenDescriptor, RegistryError> {
    workspace_screen(&WorkspaceSpec {
        id: ScreenId::Actions,
        title: "Actions",
        route: "actions",
        list: ACTIONS_LIST_PANEL,
        detail: "action-detail",
        banner: "action-list-banner",
        filter: "action-list-filter",
        // The actions screen loads its run detail on demand rather than
        // following the list selection, so it declares no coupling.
        subject_owner: None,
        subject_type: None,
    })
}
