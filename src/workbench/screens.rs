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

use crate::domain::action_registry::HandlerKey;
use crate::domain::default_action_inventory::{InventoryError, compiled_inventory};
use crate::domain::input_context::{ContextId, ContextIdError};
use crate::host_controls::ControlKind;

use super::activation::ScreenBinding;
use super::config::insets_config;
use super::descriptor::{
    Axis, HostPanelCapability, HostPanelModelSource, LayoutChild, LayoutNode, OverlayKind,
    PanelDescriptor, ScreenDescriptor, Size,
};
use super::geometry::Insets;
use super::ids::{IdError, MAX_SCREENS, PanelId, PanelTypeId, RouteId, ScreenId, ScreenIdentity};
use super::panel_types::FILTER_BAND_PANEL_TYPE;

pub use super::screens_ports::{SELECTION_PORT, SUBJECT_PORT};
use super::validate::{DescriptorError, validate_descriptor};
fn dashboard_bindings() -> Result<Vec<ScreenBinding>, RegistryError> {
    let dashboard = ContextId::parse("dashboard")?;
    let inventory = compiled_inventory()?;
    let help_action = inventory
        .actions
        .iter()
        .find(|action| {
            action.handler == HandlerKey::OpenHelp && action.contexts.contains(&dashboard)
        })
        .map(|action| action.id.clone())
        .ok_or(RegistryError::MissingDashboardContextBinding)?;
    let binding = inventory
        .bindings
        .into_iter()
        .find(|binding| binding.context == dashboard && binding.action == help_action)
        .ok_or(RegistryError::MissingDashboardContextBinding)?;
    Ok(vec![ScreenBinding {
        context: binding.context,
        action: binding.action,
    }])
}

pub(super) const HOST_OVERLAYS: [OverlayKind; 3] = OverlayKind::ALL;

/// Panel type whose visible content rectangle drives a live PTY.
///
/// The resolver guarantees a visible panel of this type always receives a
/// nonzero content rectangle (CW04-08); it is hidden, or the screen falls back
/// to the too-small layout, rather than a PTY being sized to zero.
pub const PTY_PANEL_TYPE: &str = "pty-terminal";

/// Identity of the repository sidebar, which every workspace screen shares.
pub const REPOSITORIES_PANEL: &str = "repositories";

/// The dashboard's zero-agent Agent Types availability pane (issue #734).
///
/// Named here because the application, not the descriptor, decides which side
/// of the dashboard is showing; `screen_layout::hidden_panel_ids` addresses
/// this panel by identity.
pub const AGENT_TYPES_PANEL: &str = "agent-types";

// ── Shipped geometry constants ─────────────────────────────────────────────
//
// These mirror the widths and proportions the screens render today.

/// Columns reserved for the repository sidebar.
pub(super) const SIDEBAR_COLUMNS: u16 = 22;
/// Columns reserved for the dashboard preview pane.
const PREVIEW_COLUMNS: u16 = 36;
/// Columns reserved for the Settings section list.
const SETTINGS_SECTIONS_COLUMNS: u16 = 20;
/// Rows the dashboard search input row occupies when shown.
const SEARCH_ROW_ROWS: u16 = 1;
/// Rows the split-screen filter band occupies.
const SPLIT_FILTER_ROWS: u16 = 3;

/// The STATUS block: the bordered pane's vertical chrome — top border plus
/// title row (2) and bottom border (1) — plus one row per bucket (4), so
/// 2 + 1 + 4 = 7 and the pane's interior fits all four buckets. The legacy
/// borderless rail spent its first row on a header; the pane carries STATUS
/// on its border now, so every interior row is a bucket.
const STATUS_BLOCK_ROWS: u16 =
    LIST_PANE_CHROME.top + LIST_PANE_CHROME.bottom + STATUS_BLOCK_BUCKETS;
/// One row per STATUS bucket: Needs you, Working, Ready, Stale.
const STATUS_BLOCK_BUCKETS: u16 = 4;
/// Rows a workspace error/notice banner occupies when shown.
pub(super) const BANNER_ROWS: u16 = 1;
/// Rows the workspace filter-controls band occupies when open.
pub(super) const FILTER_CONTROLS_ROWS: u16 = 6;
/// Weight of the workspace list pane; the list takes three tenths.
pub(super) const LIST_WEIGHT: u16 = 3;
/// Weight of the workspace detail pane; the detail takes seven tenths.
pub(super) const DETAIL_WEIGHT: u16 = 7;
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
pub(super) const LIST_MIN_ROWS: u16 = 0;
/// The workspace detail reserves nothing; it is a pure seven-tenths share.
pub(super) const DETAIL_MIN_ROWS: u16 = 0;
/// A flexible column reserves nothing; it takes what the fixed columns leave.
pub(super) const FLEX_MIN_COLUMNS: u16 = 0;

// ── Shipped chrome ─────────────────────────────────────────────────────────

/// Bordered list pane: top border + title row, side borders, bottom border.
pub(super) const LIST_PANE_CHROME: Insets = Insets::new(2, 1, 1, 1);
/// Detail pane: border plus one column of content padding per side.
pub(super) const DETAIL_PANE_CHROME: Insets = Insets::new(1, 1, 2, 2);
/// Repository sidebar: border, title, and one column of content padding.
const SIDEBAR_CHROME: Insets = Insets::new(3, 1, 2, 2);
/// Preview pane: border, title, and one column of content padding.
const PREVIEW_CHROME: Insets = Insets::new(3, 1, 2, 2);
/// Embedded terminal view: border plus header row.
const TERMINAL_CHROME: Insets = Insets::new(2, 1, 1, 1);
/// Unbordered single-row band (search input, error banner).
pub(super) const BAND_CHROME: Insets = Insets::new(0, 0, 1, 0);
/// Bordered band (filter controls).
pub(super) const BORDERED_BAND_CHROME: Insets = Insets::new(1, 1, 1, 1);

/// Why a compiled screen table could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// A compiled identifier violates the identifier grammar.
    Id(IdError),
    /// The immutable compiled action inventory is invalid.
    ActionInventory(InventoryError),
    /// A compiled input context violates the context grammar.
    ContextId(ContextIdError),
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
    /// The canonical action inventory has no Dashboard context marker.
    MissingDashboardContextBinding,
}

impl From<IdError> for RegistryError {
    fn from(error: IdError) -> Self {
        Self::Id(error)
    }
}

impl From<InventoryError> for RegistryError {
    fn from(error: InventoryError) -> Self {
        Self::ActionInventory(error)
    }
}

impl From<ContextIdError> for RegistryError {
    fn from(error: ContextIdError) -> Self {
        Self::ContextId(error)
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
            Self::ActionInventory(error) => write!(formatter, "{error}"),
            Self::ContextId(error) => {
                write!(formatter, "invalid compiled input context: {error}")
            }
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
            Self::MissingDashboardContextBinding => {
                formatter.write_str("canonical action inventory has no Dashboard context marker")
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

/// Exact selected-provider ownership for one lowered screen panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePanelBinding {
    /// Lowered local or package screen that places the panel.
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
    /// [`ScreenIdentity`], so an unrecognised value can never become an identity
    /// no published descriptor backs.
    #[must_use]
    pub fn resolve(&self, value: &str) -> Option<ScreenIdentity> {
        self.screens
            .iter()
            .find(|screen| screen.id.as_str() == value)
            .map(|screen| screen.id)
    }

    /// The screen selected when no valid prior screen is known.
    #[must_use]
    pub fn initial_screen(&self) -> Option<&ScreenDescriptor> {
        self.screens.first()
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
        super::screens_github::issues_screen()?,
        super::screens_github::pull_requests_screen()?,
        super::screens_github::actions_screen()?,
        errors_screen()?,
        terminals_screen()?,
        settings_screen()?,
    ])
}

// ── Construction helpers ───────────────────────────────────────────────────

/// How many shipped screens are declared builtin descriptors rather than
/// residual compiled adapters. Tests key shipped-screen counts off this so a
/// migration flips one number instead of a dozen literals.
pub const SHIPPED_BUILTIN_SCREENS: usize = 3;

/// A weighted share of the cells left once every minimum is satisfied.
///
/// Zero is coerced to one because a weight of zero would silently mean "claim
/// nothing", which is expressed by visibility rather than by sizing.
pub(super) fn weight(value: u16) -> Size {
    Size::Weight(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

/// A fixed claim of `value` cells; zero is coerced to one because a fixed size
/// of zero is not representable and would mean "hidden", which is expressed by
/// visibility instead.
fn fixed(value: u16) -> Size {
    Size::Fixed(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

pub(super) fn panel(
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
        host_capability: None,
        focusable,
        required,
        ports: Vec::new(),
    })
}

fn host_panel(
    id: &'static str,
    panel_type: &'static str,
    model_source: HostPanelModelSource,
    control_kind: ControlKind,
    (focusable, required): (bool, bool),
    chrome: Insets,
) -> Result<PanelDescriptor, IdError> {
    let mut descriptor = panel(id, panel_type, focusable, required, chrome)?;
    descriptor.host_capability = Some(HostPanelCapability::compiled(model_source, control_kind));
    Ok(descriptor)
}

/// The repository sidebar, which every workspace screen shares.
pub(super) fn sidebar_panel() -> Result<PanelDescriptor, IdError> {
    host_panel(
        REPOSITORIES_PANEL,
        "repository-list",
        HostPanelModelSource::RepositoryList,
        ControlKind::List,
        (true, true),
        SIDEBAR_CHROME,
    )
}

pub(super) fn leaf(id: &'static str) -> Result<LayoutNode, IdError> {
    Ok(LayoutNode::Leaf {
        panel: PanelId::parse(id)?,
    })
}

/// A child that is never hidden by the resolver.
pub(super) fn required_child(node: LayoutNode, size: Size, min: u16) -> LayoutChild {
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
pub(super) fn fixed_child(node: LayoutNode, cells: u16) -> LayoutChild {
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
pub(super) fn collapsible_child(
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
pub(super) fn band_child(node: LayoutNode, rows: u16, collapse_priority: i32) -> LayoutChild {
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

pub(super) fn column(children: Vec<LayoutChild>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Vertical,
        gap: NO_GAP,
        children,
    }
}

pub(super) fn row(children: Vec<LayoutChild>) -> LayoutNode {
    LayoutNode::Split {
        axis: Axis::Horizontal,
        gap: NO_GAP,
        children,
    }
}

pub(super) fn focus_order(ids: &[&'static str]) -> Result<Vec<PanelId>, IdError> {
    ids.iter().copied().map(PanelId::parse).collect()
}

// ── Shipped screens ────────────────────────────────────────────────────────

fn dashboard_panels() -> Result<Vec<PanelDescriptor>, IdError> {
    Ok(vec![
        sidebar_panel()?,
        host_panel(
            "search",
            "search-input",
            HostPanelModelSource::SearchInput,
            ControlKind::Form,
            (false, false),
            BAND_CHROME,
        )?,
        host_panel(
            "agents",
            "agent-list",
            HostPanelModelSource::AgentList,
            ControlKind::List,
            (true, false),
            LIST_PANE_CHROME,
        )?,
        host_panel(
            AGENT_TYPES_PANEL,
            "agent-types-status",
            HostPanelModelSource::AgentTypeAvailability,
            ControlKind::List,
            (true, false),
            LIST_PANE_CHROME,
        )?,
        panel("terminal", PTY_PANEL_TYPE, true, true, TERMINAL_CHROME)?,
        host_panel(
            "preview",
            "agent-preview",
            HostPanelModelSource::AgentPreview,
            ControlKind::Detail,
            (false, false),
            PREVIEW_CHROME,
        )?,
    ])
}

/// `core.dashboard` — sidebar, agent list over the embedded terminal, and preview.
/// The search row is a band shown only while the Dashboard filter is focused or active.
fn dashboard_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: crate::workbench::DASHBOARD_IDENTITY,
        title: "LLxprt Jefe".to_owned(),
        route: RouteId::parse("dashboard")?,
        panels: dashboard_panels()?,
        initial_focus: PanelId::parse(REPOSITORIES_PANEL)?,
        // The availability pane is the zero-agent form of the agent list, so
        // it takes the agent list's place in the traversal. Focus cycling
        // filters the order by resolved visibility, so exactly one of the two
        // forms is ever reachable and the pane's cursor can move while it is
        // the pane on screen (#734).
        focus_order: focus_order(&[REPOSITORIES_PANEL, "agents", AGENT_TYPES_PANEL, "terminal"])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: vec![
            super::descriptor::HostScreenCapability::DashboardActionContext,
            super::descriptor::HostScreenCapability::DashboardFooter,
        ],
        bindings: dashboard_bindings()?,
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
                    // The zero-agent form of the workspace, declared beside the
                    // form it replaces rather than under a wrapping split: a
                    // hidden child receives no cells, so whichever form is
                    // showing takes the whole flexible width from the one
                    // resolver, and the tree stays shallow enough to survive a
                    // settings layout override round trip (#734).
                    required_child(leaf(AGENT_TYPES_PANEL)?, weight(1), FLEX_MIN_COLUMNS),
                ]),
                weight(1),
                TERMINAL_MIN_ROWS,
            ),
        ]),
    })
}

/// `core.repositories` — the split view: the repository list over its STATUS
/// block in the fixed left rail, the agent card grid beside them under the
/// filter band.
fn repositories_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: crate::workbench::REPOSITORIES_IDENTITY,
        title: "Repositories".to_owned(),
        route: RouteId::parse(REPOSITORIES_ROUTE)?,
        panels: vec![
            sidebar_panel()?,
            host_panel(
                "status",
                "status-block",
                HostPanelModelSource::WorkbenchStatus,
                ControlKind::List,
                (true, false),
                LIST_PANE_CHROME,
            )?,
            host_panel(
                "cards",
                "workbench-cards",
                HostPanelModelSource::WorkbenchCards,
                ControlKind::List,
                (true, false),
                LIST_PANE_CHROME,
            )?,
            panel("filter", FILTER_BAND_PANEL_TYPE, false, false, BAND_CHROME)?,
        ],
        initial_focus: PanelId::parse(REPOSITORIES_PANEL)?,
        focus_order: focus_order(&[REPOSITORIES_PANEL, "status", "cards"])?,
        relationships: Vec::new(),
        activation: Vec::new(),
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: Vec::new(),
        bindings: Vec::new(),
        layout: column(vec![
            band_child(leaf("filter")?, SPLIT_FILTER_ROWS, -100),
            required_child(
                row(vec![
                    fixed_child(
                        column(vec![
                            required_child(leaf(REPOSITORIES_PANEL)?, weight(1), LIST_MIN_ROWS),
                            fixed_child(leaf("status")?, STATUS_BLOCK_ROWS),
                        ]),
                        SIDEBAR_COLUMNS,
                    ),
                    required_child(leaf("cards")?, weight(1), FLEX_MIN_COLUMNS),
                ]),
                weight(1),
                LIST_MIN_ROWS,
            ),
        ]),
    })
}

/// The route a screen is reached through.
///
/// Compiled as a total function for the same reason as [`initial_focus`]:
/// rooting a session must not depend on a lookup that can fail.
/// `route_agrees_with_every_descriptor` keeps it honest.
#[must_use]
pub const fn route_of(screen: ScreenId) -> RouteId {
    RouteId::from_static(match screen {
        ScreenId::Issues => "issues",
        ScreenId::PullRequests => "pull-requests",
        ScreenId::Actions => "actions",
        ScreenId::Errors => "errors",
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
        ScreenId::Issues => ISSUES_LIST_PANEL,
        ScreenId::PullRequests => PULL_REQUESTS_LIST_PANEL,
        ScreenId::Actions => ACTIONS_LIST_PANEL,
        ScreenId::Errors => ERRORS_LIST_PANEL,
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
/// Route literal the Terminal Manager screen is reachable through.
pub const TERMINALS_ROUTE: &str = "terminals";
/// Route literal the Repositories (split) screen is reachable through.
pub const REPOSITORIES_ROUTE: &str = "repositories";
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
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: Vec::new(),
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
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: Vec::new(),
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
        id: crate::workbench::TERMINALS_IDENTITY,
        title: "Terminals".to_owned(),
        route: RouteId::parse(TERMINALS_ROUTE)?,
        panels: vec![
            sidebar_panel()?,
            host_panel(
                "shell-list",
                "shell-list",
                HostPanelModelSource::SessionList,
                ControlKind::List,
                (true, true),
                LIST_PANE_CHROME,
            )?,
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
        overlays: HOST_OVERLAYS.to_vec(),
        host_capabilities: Vec::new(),
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
