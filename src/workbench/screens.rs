//! The compiled screen descriptors (issue #384, CW04-01).
//!
//! These constructors are the *sole* definition of each shipped screen. There
//! is no external syntax, no override source, and no second place a screen's
//! panel set, focus order, or layout is described. A renderer that wants to
//! know what a screen contains asks the registry.
//!
//! Every descriptor is validated before it enters the registry, so a compiled
//! mistake fails in tests and at startup rather than producing a half-formed
//! screen at render time.
//!
//! # Why seven screens
//!
//! Every screen the application can display has a descriptor, because the
//! registry replaces the legacy screen enum outright. Five of them
//! (`core.dashboard`, `core.repositories`, `github.issues`,
//! `github.pull-requests`, `github.actions`) carry the parity guarantees in
//! issue #384; `core.errors` and `core.terminals` are the remaining live
//! screens, which must have stable identities for the legacy enum to be
//! deletable at all.
//!
//! # Why the panel sets look the way they do
//!
//! Each descriptor mirrors the screen that exists today, verified against its
//! renderer. In particular the repository sidebar is a real, focusable panel on
//! every workspace screen (`IssueFocus::RepoList`, `PrFocus::RepoList`,
//! `ActionsFocus::RepoList`, `ErrorsFocus::RepoList`), and the conditional
//! banner and filter bands are real rows that shift every pane below them, so
//! both are modelled as panels the application shows or hides. Leaving either
//! out would change behavior, which the parity requirement forbids.

use std::num::NonZeroU16;

use super::config::insets_config;
use super::descriptor::{
    Axis, LayoutChild, LayoutNode, PanelDescriptor, PortDirection, ScreenDescriptor, Size,
};
use super::geometry::Insets;
use super::ids::{IdError, MAX_SCREENS, PanelId, PanelTypeId, RouteId, ScreenId, ScreenIdentity};
use super::screens_ports::{selection_port, subject_port, workspace_relationships};

pub use super::screens_ports::{SELECTION_PORT, SUBJECT_PORT};
use super::validate::{DescriptorError, validate_descriptor};

/// Panel type whose visible content rectangle drives a live PTY.
///
/// The resolver guarantees a visible panel of this type always receives a
/// nonzero content rectangle (CW04-08); it is hidden, or the screen falls back
/// to the too-small layout, rather than a PTY being sized to zero.
pub const PTY_PANEL_TYPE: &str = "pty-terminal";

/// Identity of the repository sidebar, which every workspace screen shares.
pub const REPOSITORIES_PANEL: &str = "repositories";

// ── Shipped geometry constants ─────────────────────────────────────────────
//
// These mirror the widths and proportions the screens render today.

/// Columns reserved for the repository sidebar.
const SIDEBAR_COLUMNS: u16 = 22;
/// Columns reserved for the dashboard preview pane.
const PREVIEW_COLUMNS: u16 = 36;
/// Columns reserved for the Settings section list.
const SETTINGS_SECTIONS_COLUMNS: u16 = 20;
/// Rows the dashboard search input row occupies when shown.
const SEARCH_ROW_ROWS: u16 = 1;
/// Rows the split-screen filter band occupies.
const SPLIT_FILTER_ROWS: u16 = 3;
/// Rows a workspace error/notice banner occupies when shown.
const BANNER_ROWS: u16 = 1;
/// Rows the workspace filter-controls band occupies when open.
const FILTER_CONTROLS_ROWS: u16 = 6;
/// Weight of the workspace list pane; the list takes three tenths.
const LIST_WEIGHT: u16 = 3;
/// Weight of the workspace detail pane; the detail takes seven tenths.
const DETAIL_WEIGHT: u16 = 7;
/// Weight of the dashboard agent pane; the agent list takes a quarter.
const AGENT_WEIGHT: u16 = 1;
/// Weight of the dashboard terminal pane; the terminal takes three quarters.
const TERMINAL_WEIGHT: u16 = 3;

// A minimum is charged *before* weights are applied, so declaring one changes
// the proportion a pane actually receives. The shipped screens reserve nothing
// for their flexible panes — they are pure proportions that shrink until the
// resolver hides a degenerate pane — so these are zero, and the pane sizes match
// what the screens render today. Declaring a comfortable-looking minimum here
// would silently skew every split away from its declared weight.

/// The agent pane reserves nothing; it is a pure quarter share.
const AGENT_MIN_ROWS: u16 = 0;
/// The terminal pane reserves nothing; it is a pure three-quarter share.
const TERMINAL_MIN_ROWS: u16 = 0;
/// The workspace list reserves nothing; it is a pure three-tenths share.
const LIST_MIN_ROWS: u16 = 0;
/// The workspace detail reserves nothing; it is a pure seven-tenths share.
const DETAIL_MIN_ROWS: u16 = 0;
/// A flexible column reserves nothing; it takes what the fixed columns leave.
const FLEX_MIN_COLUMNS: u16 = 0;

// ── Shipped chrome ─────────────────────────────────────────────────────────

/// Bordered list pane: top border + title row, side borders, bottom border.
const LIST_PANE_CHROME: Insets = Insets::new(2, 1, 1, 1);
/// Detail pane: border plus one column of content padding per side.
const DETAIL_PANE_CHROME: Insets = Insets::new(1, 1, 2, 2);
/// Repository sidebar: border, title, and one column of content padding.
const SIDEBAR_CHROME: Insets = Insets::new(3, 1, 2, 2);
/// Preview pane: border, title, and one column of content padding.
const PREVIEW_CHROME: Insets = Insets::new(3, 1, 2, 2);
/// Embedded terminal view: border plus header row.
const TERMINAL_CHROME: Insets = Insets::new(2, 1, 1, 1);
/// Unbordered single-row band (search input, error banner).
const BAND_CHROME: Insets = Insets::new(0, 0, 1, 0);
/// Bordered band (filter controls).
const BORDERED_BAND_CHROME: Insets = Insets::new(1, 1, 1, 1);

/// Why a compiled screen table could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// A compiled identifier violates the identifier grammar.
    Id(IdError),
    /// A compiled descriptor violates a structural invariant.
    Descriptor(DescriptorError),
    /// Two compiled descriptors share one screen identity.
    DuplicateScreen {
        /// The repeated screen identity.
        screen: String,
    },
    /// The table declares more than [`MAX_SCREENS`] screens.
    TooManyScreens {
        /// Declared screen count.
        count: usize,
    },
    /// A screen has no compiled descriptor.
    MissingScreen {
        /// The screen with no descriptor.
        screen: ScreenId,
    },
}

impl From<IdError> for RegistryError {
    fn from(error: IdError) -> Self {
        Self::Id(error)
    }
}

impl From<DescriptorError> for RegistryError {
    fn from(error: DescriptorError) -> Self {
        Self::Descriptor(error)
    }
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(error) => write!(formatter, "invalid compiled identifier: {error}"),
            Self::Descriptor(error) => write!(formatter, "invalid compiled descriptor: {error}"),
            Self::DuplicateScreen { screen } => {
                write!(formatter, "screen {screen} is declared twice")
            }
            Self::TooManyScreens { count } => {
                write!(formatter, "{count} screens declared (max {MAX_SCREENS})")
            }
            Self::MissingScreen { screen } => {
                write!(formatter, "screen {screen} has no compiled descriptor")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// A validated, order-stable set of screen descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRegistry {
    screens: Vec<ScreenDescriptor>,
    panel_bindings: Vec<PackagePanelBinding>,
}

/// Exact selected-package ownership for one lowered screen panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePanelBinding {
    /// Lowered package screen that contains the panel.
    pub screen: ScreenIdentity,
    /// Panel identity within the screen.
    pub panel: PanelId,
    /// Selected package/provider owner.
    pub owner: crate::domain::Id,
    /// Owner-qualified manifest panel type.
    pub panel_type: crate::domain::Id,
    /// Model kinds this selected manifest permits.
    pub model_kinds: Vec<crate::domain::plugin::ModelKind>,
    /// Semantic events this selected manifest permits.
    pub event_schema: Vec<crate::domain::plugin::EventSchemaEntry>,
    /// Owner-declared action ids that snapshot affordances may reference.
    pub action_authority: Vec<crate::domain::action_registry::ActionId>,
}

impl ScreenRegistry {
    /// Build a registry, validating every descriptor and rejecting duplicates.
    ///
    /// # Errors
    ///
    /// Returns the first structural violation found.
    pub fn new(screens: Vec<ScreenDescriptor>) -> Result<Self, RegistryError> {
        Self::with_panel_bindings(screens, Vec::new())
    }

    /// Build a registry with explicit selected-package panel ownership.
    ///
    /// # Errors
    ///
    /// Returns the first structural screen violation found.
    pub fn with_panel_bindings(
        screens: Vec<ScreenDescriptor>,
        panel_bindings: Vec<PackagePanelBinding>,
    ) -> Result<Self, RegistryError> {
        if screens.len() > MAX_SCREENS {
            return Err(RegistryError::TooManyScreens {
                count: screens.len(),
            });
        }
        for (index, screen) in screens.iter().enumerate() {
            validate_descriptor(screen)?;
            if screens[..index].iter().any(|prior| prior.id == screen.id) {
                return Err(RegistryError::DuplicateScreen {
                    screen: screen.id.as_str().to_owned(),
                });
            }
        }
        Ok(Self {
            screens,
            panel_bindings,
        })
    }

    /// Every descriptor in declaration order.
    #[must_use]
    pub fn screens(&self) -> &[ScreenDescriptor] {
        &self.screens
    }

    /// Look up one compiled screen's descriptor.
    #[must_use]
    pub fn get(&self, id: ScreenId) -> Option<&ScreenDescriptor> {
        self.get_identity(ScreenIdentity::Compiled(id))
    }

    /// Look up one descriptor by stable identity, compiled or lowered.
    #[must_use]
    pub fn get_identity(&self, id: ScreenIdentity) -> Option<&ScreenDescriptor> {
        self.screens.iter().find(|screen| screen.id == id)
    }

    /// Exact package/provider binding for one panel on one lowered screen.
    #[must_use]
    pub fn panel_binding(
        &self,
        screen: ScreenIdentity,
        panel: &PanelId,
    ) -> Option<&PackagePanelBinding> {
        self.panel_bindings
            .iter()
            .find(|binding| binding.screen == screen && &binding.panel == panel)
    }

    /// Resolve a routable screen from text that came from outside the program.
    ///
    /// This is the only way a persisted or otherwise external value becomes a
    /// [`ScreenId`], so an unrecognised value can never become an identity no
    /// descriptor backs. A lowered custom screen is deliberately not resolvable
    /// here: it has a descriptor but no renderer or route, so restoring a
    /// session onto it would open a screen nothing can draw.
    #[must_use]
    pub fn resolve(&self, value: &str) -> Option<ScreenId> {
        self.screens
            .iter()
            .find(|screen| screen.id.as_str() == value)
            .and_then(|screen| screen.id.compiled())
    }

    /// The screen selected when no valid prior screen is known.
    ///
    /// Only a compiled screen can be the fallback, because the fallback must
    /// always be renderable and routable.
    #[must_use]
    pub fn initial_screen(&self) -> Option<&ScreenDescriptor> {
        self.screens
            .iter()
            .find(|screen| screen.id.compiled().is_some())
    }
}

/// Build the shipped screen table.
///
/// # Errors
///
/// Returns a [`RegistryError`] if any compiled descriptor is malformed. This is
/// a programming error in this module and is surfaced at startup and in tests
/// rather than tolerated at render time.
pub fn builtin_screens() -> Result<ScreenRegistry, RegistryError> {
    ScreenRegistry::new(vec![
        dashboard_screen()?,
        repositories_screen()?,
        issues_screen()?,
        pull_requests_screen()?,
        actions_screen()?,
        errors_screen()?,
        terminals_screen()?,
        settings_screen()?,
    ])
}

// ── Construction helpers ───────────────────────────────────────────────────

/// A weighted share of the cells left once every minimum is satisfied.
///
/// Zero is coerced to one because a weight of zero would silently mean "claim
/// nothing", which is expressed by visibility rather than by sizing.
fn weight(value: u16) -> Size {
    Size::Weight(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

/// A fixed claim of `value` cells; zero is coerced to one because a fixed size
/// of zero is not representable and would mean "hidden", which is expressed by
/// visibility instead.
fn fixed(value: u16) -> Size {
    Size::Fixed(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

fn panel(
    id: &'static str,
    panel_type: &'static str,
    focusable: bool,
    required: bool,
    chrome: Insets,
) -> Result<PanelDescriptor, IdError> {
    Ok(PanelDescriptor {
        id: PanelId::parse(id)?,
        panel_type: PanelTypeId::parse(panel_type)?,
        config: insets_config(chrome).ok_or(IdError::InvalidByte)?,
        focusable,
        required,
        ports: Vec::new(),
    })
}

/// The repository sidebar, which every workspace screen shares.
fn sidebar_panel() -> Result<PanelDescriptor, IdError> {
    panel(
        REPOSITORIES_PANEL,
        "repository-list",
        true,
        true,
        SIDEBAR_CHROME,
    )
}

fn leaf(id: &'static str) -> Result<LayoutNode, IdError> {
    Ok(LayoutNode::Leaf {
        panel: PanelId::parse(id)?,
    })
}

/// A child that is never hidden by the resolver.
fn required_child(node: LayoutNode, size: Size, min: u16) -> LayoutChild {
    LayoutChild {
        node,
        size,
        min,
        max: None,
        collapsible: false,
        collapse_priority: None,
    }
}

/// A child pinned to an exact cell count.
fn fixed_child(node: LayoutNode, cells: u16) -> LayoutChild {
    LayoutChild {
        node,
        size: fixed(cells),
        min: cells,
        max: Some(cells),
        collapsible: false,
        collapse_priority: None,
    }
}

/// A child the resolver may hide to fit its siblings.
fn collapsible_child(
    node: LayoutNode,
    size: Size,
    min: u16,
    collapse_priority: i32,
) -> LayoutChild {
    LayoutChild {
        node,
        size,
        min,
        max: None,
        collapsible: true,
        collapse_priority: Some(collapse_priority),
    }
}

/// A fixed-height band the resolver may hide before anything else.
fn band_child(node: LayoutNode, rows: u16, collapse_priority: i32) -> LayoutChild {
    LayoutChild {
        node,
        size: fixed(rows),
        min: rows,
        max: Some(rows),
        collapsible: true,
        collapse_priority: Some(collapse_priority),
    }
}

/// Panes draw their own border inside their rectangle, so shipped splits leave
/// no gap between children.
const NO_GAP: u16 = 0;

fn column(children: Vec<LayoutChild>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Vertical,
        gap: NO_GAP,
        children,
    }
}

fn row(children: Vec<LayoutChild>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Horizontal,
        gap: NO_GAP,
        children,
    }
}

fn focus_order(ids: &[&'static str]) -> Result<Vec<PanelId>, IdError> {
    ids.iter().copied().map(PanelId::parse).collect()
}

// ── Shipped screens ────────────────────────────────────────────────────────

/// `core.dashboard` — sidebar, agent list over the embedded terminal, preview.
///
/// The search row appears only while the dashboard filter is focused or
/// active, so it is a band the application shows and hides.
fn dashboard_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Dashboard),
        title: "Dashboard".to_owned(),
        route: RouteId::parse("dashboard")?,
        panels: vec![
            sidebar_panel()?,
            panel("search", "search-input", false, false, BAND_CHROME)?,
            panel("agents", "agent-list", true, false, LIST_PANE_CHROME)?,
            panel("terminal", PTY_PANEL_TYPE, true, true, TERMINAL_CHROME)?,
            panel("preview", "agent-preview", false, false, PREVIEW_CHROME)?,
        ],
        initial_focus: PanelId::parse(REPOSITORIES_PANEL)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, "agents", "terminal"])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: column(vec![
            band_child(leaf("search")?, SEARCH_ROW_ROWS, -100),
            required_child(
                row(vec![
                    fixed_child(leaf(REPOSITORIES_PANEL)?, SIDEBAR_COLUMNS),
                    required_child(
                        column(vec![
                            collapsible_child(
                                leaf("agents")?,
                                weight(AGENT_WEIGHT),
                                AGENT_MIN_ROWS,
                                0,
                            ),
                            required_child(
                                leaf("terminal")?,
                                weight(TERMINAL_WEIGHT),
                                TERMINAL_MIN_ROWS,
                            ),
                        ]),
                        weight(1),
                        FLEX_MIN_COLUMNS,
                    ),
                    collapsible_child(
                        leaf("preview")?,
                        fixed(PREVIEW_COLUMNS),
                        PREVIEW_COLUMNS,
                        -1,
                    ),
                ]),
                weight(1),
                TERMINAL_MIN_ROWS,
            ),
        ]),
    })
}

/// `core.repositories` — the split view: the repository list under its filter
/// band, occupying the full width.
fn repositories_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Repositories),
        title: "Repositories".to_owned(),
        route: RouteId::parse("repositories")?,
        panels: vec![
            sidebar_panel()?,
            panel("filter", "filter-band", false, false, BAND_CHROME)?,
        ],
        initial_focus: PanelId::parse(REPOSITORIES_PANEL)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: column(vec![
            band_child(leaf("filter")?, SPLIT_FILTER_ROWS, -100),
            required_child(leaf(REPOSITORIES_PANEL)?, weight(1), LIST_MIN_ROWS),
        ]),
    })
}

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
    /// Versioned type the list publishes and the detail consumes, when the
    /// screen couples them.
    ///
    /// `None` means the screen's detail pane is not driven by its list
    /// selection, so it declares no ports and no relationship.
    subject_type: Option<&'static str>,
}

/// Attach an optional port to a panel.
fn ported(
    mut panel: PanelDescriptor,
    port: Option<super::descriptor::PortDescriptor>,
) -> PanelDescriptor {
    panel.ports.extend(port);
    panel
}

/// The route a screen is reached through.
///
/// Compiled as a total function for the same reason as [`initial_focus`]:
/// rooting a session must not depend on a lookup that can fail.
/// `route_agrees_with_every_descriptor` keeps it honest.
#[must_use]
pub const fn route_of(screen: ScreenId) -> RouteId {
    RouteId::from_static(match screen {
        ScreenId::Dashboard => "dashboard",
        ScreenId::Repositories => "repositories",
        ScreenId::Issues => "issues",
        ScreenId::PullRequests => "pull-requests",
        ScreenId::Actions => "actions",
        ScreenId::Errors => "errors",
        ScreenId::Terminals => "terminals",
        ScreenId::Settings => "settings",
    })
}

/// The panel a screen focuses when an instance of it is first created.
///
/// Compiled as a total function rather than read back out of the registry, so
/// creating a screen instance cannot fail on a registry lookup — there is no
/// "what if the descriptor is missing" branch to get wrong at the moment the
/// session moves. `initial_focus_agrees_with_every_descriptor` keeps this and
/// the descriptors from drifting apart.
#[must_use]
pub const fn initial_focus(screen: ScreenId) -> PanelId {
    PanelId::from_static(match screen {
        ScreenId::Dashboard | ScreenId::Repositories => REPOSITORIES_PANEL,
        ScreenId::Issues => ISSUES_LIST_PANEL,
        ScreenId::PullRequests => PULL_REQUESTS_LIST_PANEL,
        ScreenId::Actions => ACTIONS_LIST_PANEL,
        ScreenId::Errors => ERRORS_LIST_PANEL,
        ScreenId::Terminals => TERMINALS_LIST_PANEL,
        ScreenId::Settings => SETTINGS_SECTIONS_PANEL,
    })
}

/// Identity of the issues list panel.
pub const ISSUES_LIST_PANEL: &str = "issue-list";
/// Identity of the pull-requests list panel.
pub const PULL_REQUESTS_LIST_PANEL: &str = "pr-list";
/// Identity of the workflow-runs list panel.
pub const ACTIONS_LIST_PANEL: &str = "action-list";
/// Identity of the errors list panel.
pub const ERRORS_LIST_PANEL: &str = "error-list";
/// Identity of the terminal-manager list panel.
pub const TERMINALS_LIST_PANEL: &str = "shell-list";
/// Identity of the Settings section list.
pub const SETTINGS_SECTIONS_PANEL: &str = "settings-sections";
/// Identity of the Settings General panel.
pub const SETTINGS_GENERAL_PANEL: &str = "settings-general";
/// Identity of the Settings Appearance panel.
pub const SETTINGS_APPEARANCE_PANEL: &str = "settings-appearance";
/// Identity of the Settings Agent Types panel.
pub const SETTINGS_AGENT_TYPES_PANEL: &str = "settings-agent-types";
/// Identity of the Settings Screens panel.
pub const SETTINGS_SCREENS_PANEL: &str = "settings-screens";
/// Identity of the Settings Keys panel.
pub const SETTINGS_KEYS_PANEL: &str = "settings-keys";
/// Identity of the Settings Plugins panel.
pub const SETTINGS_PLUGINS_PANEL: &str = "settings-plugins";
/// Identity of the Settings Diagnostics panel.
pub const SETTINGS_DIAGNOSTICS_PANEL: &str = "settings-diagnostics";

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
                "filter-band",
                false,
                false,
                BORDERED_BAND_CHROME,
            )?,
            ported(
                panel(spec.list, spec.list, true, true, LIST_PANE_CHROME)?,
                selection_port(spec.subject_type, PortDirection::Output)?,
            ),
            ported(
                panel(spec.detail, spec.detail, true, false, DETAIL_PANE_CHROME)?,
                subject_port(spec.subject_type, PortDirection::Input)?,
            ),
        ],
        initial_focus: PanelId::parse(spec.list)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, spec.list, spec.detail])?,
        relationships: workspace_relationships(spec.list, spec.detail, spec.subject_type)?,
        activation: Vec::new(),
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
fn issues_screen() -> Result<ScreenDescriptor, RegistryError> {
    workspace_screen(&WorkspaceSpec {
        id: ScreenId::Issues,
        title: "Issues",
        route: "issues",
        list: ISSUES_LIST_PANEL,
        detail: "issue-detail",
        banner: "issue-list-banner",
        filter: "issue-list-filter",
        subject_type: Some("github.issue@1"),
    })
}

/// `github.pull-requests` — PR list over PR detail, which also hosts the
/// review threads, actions, and merge affordances.
fn pull_requests_screen() -> Result<ScreenDescriptor, RegistryError> {
    workspace_screen(&WorkspaceSpec {
        id: ScreenId::PullRequests,
        title: "Pull Requests",
        route: "pull-requests",
        list: PULL_REQUESTS_LIST_PANEL,
        detail: "pr-detail",
        banner: "pr-list-banner",
        filter: "pr-list-filter",
        subject_type: Some("github.pull-request@1"),
    })
}

/// `github.actions` — workflow-run list over run detail.
fn actions_screen() -> Result<ScreenDescriptor, RegistryError> {
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
        subject_type: None,
    })
}

/// `core.errors` — the error ring buffer beside the repository sidebar.
///
/// Errors mode renders no banner and no filter band, so it declares neither.
fn errors_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Errors),
        title: "Errors".to_owned(),
        route: RouteId::parse("errors")?,
        panels: vec![
            sidebar_panel()?,
            panel("error-list", "error-list", true, true, LIST_PANE_CHROME)?,
            panel(
                "error-detail",
                "error-detail",
                true,
                false,
                DETAIL_PANE_CHROME,
            )?,
        ],
        initial_focus: PanelId::parse(ERRORS_LIST_PANEL)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, "error-list", "error-detail"])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: row(vec![
            fixed_child(leaf(REPOSITORIES_PANEL)?, SIDEBAR_COLUMNS),
            required_child(
                column(vec![
                    required_child(leaf("error-list")?, weight(LIST_WEIGHT), LIST_MIN_ROWS),
                    collapsible_child(
                        leaf("error-detail")?,
                        weight(DETAIL_WEIGHT),
                        DETAIL_MIN_ROWS,
                        0,
                    ),
                ]),
                weight(1),
                FLEX_MIN_COLUMNS,
            ),
        ]),
    })
}

/// `core.settings` — the section list beside exactly one section's detail.
///
/// Each section is its own panel, not one detail pane that changes shape,
/// because the three sections carry different content and different focus
/// behaviour: General and Appearance hold editable rows, Diagnostics is a
/// read-only report. Declaring three panels and hiding the two that are not in
/// view keeps "which section is showing" a single application decision that
/// `screen_layout::hidden_panel_ids` states once.
fn settings_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Settings),
        title: "Settings".to_owned(),
        route: RouteId::parse("settings")?,
        panels: vec![
            panel(
                SETTINGS_SECTIONS_PANEL,
                "settings-sections",
                true,
                true,
                LIST_PANE_CHROME,
            )?,
            settings_section_panel(SETTINGS_GENERAL_PANEL)?,
            settings_section_panel(SETTINGS_APPEARANCE_PANEL)?,
            settings_section_panel(SETTINGS_AGENT_TYPES_PANEL)?,
            settings_section_panel(SETTINGS_SCREENS_PANEL)?,
            settings_section_panel(SETTINGS_KEYS_PANEL)?,
            settings_section_panel(SETTINGS_PLUGINS_PANEL)?,
            settings_section_panel(SETTINGS_DIAGNOSTICS_PANEL)?,
        ],
        initial_focus: PanelId::parse(SETTINGS_SECTIONS_PANEL)?,
        focus_order: focus_order(&[
            SETTINGS_SECTIONS_PANEL,
            SETTINGS_GENERAL_PANEL,
            SETTINGS_APPEARANCE_PANEL,
            SETTINGS_AGENT_TYPES_PANEL,
            SETTINGS_SCREENS_PANEL,
            SETTINGS_KEYS_PANEL,
            SETTINGS_PLUGINS_PANEL,
            SETTINGS_DIAGNOSTICS_PANEL,
        ])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: row(vec![
            fixed_child(leaf(SETTINGS_SECTIONS_PANEL)?, SETTINGS_SECTIONS_COLUMNS),
            required_child(
                column(vec![
                    settings_section_child(SETTINGS_GENERAL_PANEL, 0)?,
                    settings_section_child(SETTINGS_APPEARANCE_PANEL, 1)?,
                    settings_section_child(SETTINGS_AGENT_TYPES_PANEL, 2)?,
                    settings_section_child(SETTINGS_SCREENS_PANEL, 3)?,
                    settings_section_child(SETTINGS_KEYS_PANEL, 4)?,
                    settings_section_child(SETTINGS_PLUGINS_PANEL, 5)?,
                    settings_section_child(SETTINGS_DIAGNOSTICS_PANEL, 6)?,
                ]),
                weight(1),
                FLEX_MIN_COLUMNS,
            ),
        ]),
    })
}

/// One Settings section's detail panel.
fn settings_section_panel(id: &'static str) -> Result<PanelDescriptor, IdError> {
    panel(id, "settings-detail", true, false, DETAIL_PANE_CHROME)
}

/// One Settings section's place in the detail column.
///
/// All three share the column because exactly one is shown at a time; the
/// collapse priority orders them for the resolver rather than expressing any
/// preference about which the user sees.
fn settings_section_child(id: &'static str, order: i32) -> Result<LayoutChild, IdError> {
    Ok(collapsible_child(
        leaf(id)?,
        weight(1),
        DETAIL_MIN_ROWS,
        order,
    ))
}

/// `core.terminals` — the Terminal Manager: every runtime shell with a
/// throttled read-only preview of the selected one.
fn terminals_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Terminals),
        title: "Terminals".to_owned(),
        route: RouteId::parse("terminals")?,
        panels: vec![
            sidebar_panel()?,
            panel("shell-list", "shell-list", true, true, LIST_PANE_CHROME)?,
            panel(
                "shell-preview",
                PTY_PANEL_TYPE,
                true,
                false,
                TERMINAL_CHROME,
            )?,
        ],
        initial_focus: PanelId::parse(TERMINALS_LIST_PANEL)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, "shell-list", "shell-preview"])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        bindings: Vec::new(),
        layout: row(vec![
            fixed_child(leaf(REPOSITORIES_PANEL)?, SIDEBAR_COLUMNS),
            required_child(
                column(vec![
                    required_child(leaf("shell-list")?, weight(LIST_WEIGHT), LIST_MIN_ROWS),
                    collapsible_child(
                        leaf("shell-preview")?,
                        weight(DETAIL_WEIGHT),
                        TERMINAL_MIN_ROWS,
                        0,
                    ),
                ]),
                weight(1),
                FLEX_MIN_COLUMNS,
            ),
        ]),
    })
}
