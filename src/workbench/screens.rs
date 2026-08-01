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

use std::num::NonZeroU16;

use super::config::insets_config;
use super::descriptor::{Axis, LayoutChild, LayoutNode, PanelDescriptor, ScreenDescriptor, Size};
use super::geometry::Insets;
use super::ids::{IdError, MAX_SCREENS, PanelId, PanelTypeId, RouteId, ScreenId};
use super::validate::{DescriptorError, validate_descriptor};

/// Panel type whose visible content rectangle drives a live PTY.
///
/// The resolver guarantees a visible panel of this type always receives a
/// nonzero content rectangle (CW04-08); it is hidden rather than sized to zero.
pub const PTY_PANEL_TYPE: &str = "pty-terminal";

/// Columns reserved for the repository sidebar, matching the shipped width.
const REPOSITORY_PANEL_COLUMNS: u16 = 22;
/// Fewest rows the agent panel can occupy while keeping chrome and one row.
const AGENT_PANEL_MIN_ROWS: u16 = 3;
/// Fewest rows the terminal panel can occupy while keeping chrome and a
/// usable viewport.
const TERMINAL_PANEL_MIN_ROWS: u16 = 5;
/// Fewest rows a list panel can occupy while keeping chrome and one row.
const LIST_PANEL_MIN_ROWS: u16 = 3;
/// Fewest rows a detail panel can occupy while keeping chrome and one row.
const DETAIL_PANEL_MIN_ROWS: u16 = 3;
/// Fewest rows the pull-request actions panel can occupy.
const ACTIONS_PANEL_MIN_ROWS: u16 = 3;
/// Fewest columns any panel can occupy while keeping chrome and one column.
const PANEL_MIN_COLUMNS: u16 = 3;

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
        }
    }
}

impl std::error::Error for RegistryError {}

/// A validated, order-stable set of screen descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRegistry {
    screens: Vec<ScreenDescriptor>,
}

impl ScreenRegistry {
    /// Build a registry, validating every descriptor and rejecting duplicates.
    ///
    /// # Errors
    ///
    /// Returns the first structural violation found.
    pub fn new(screens: Vec<ScreenDescriptor>) -> Result<Self, RegistryError> {
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
        Ok(Self { screens })
    }

    /// Every descriptor in declaration order.
    #[must_use]
    pub fn screens(&self) -> &[ScreenDescriptor] {
        &self.screens
    }

    /// Look up one descriptor by stable identity.
    #[must_use]
    pub fn get(&self, id: &ScreenId) -> Option<&ScreenDescriptor> {
        self.screens.iter().find(|screen| &screen.id == id)
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
        issues_screen()?,
        pull_requests_screen()?,
        actions_screen()?,
    ])
}

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

/// Chrome of a bordered list pane: top border + title row, side borders.
const LIST_PANE_CHROME: Insets = Insets::new(2, 1, 1, 1);
/// Chrome of a bordered detail pane: border plus one column of padding.
const DETAIL_PANE_CHROME: Insets = Insets::new(1, 1, 2, 2);
/// Chrome of the repository sidebar: border, title, and content padding.
const SIDEBAR_CHROME: Insets = Insets::new(3, 1, 2, 2);
/// Chrome of the embedded terminal view: border plus header row.
const TERMINAL_CHROME: Insets = Insets::new(2, 1, 1, 1);

fn panel(
    id: &str,
    panel_type: &str,
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
    })
}

fn leaf(id: &str) -> Result<LayoutNode, IdError> {
    Ok(LayoutNode::Leaf {
        panel: PanelId::parse(id)?,
    })
}

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

fn focus_order(ids: &[&str]) -> Result<Vec<PanelId>, IdError> {
    ids.iter().map(|id| PanelId::parse(id)).collect()
}

/// `core.dashboard` — repository list with the agent list beside it.
fn dashboard_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenId::parse("core.dashboard")?,
        title: "Dashboard".to_owned(),
        route: RouteId::parse("dashboard")?,
        panels: vec![
            panel(
                "repositories",
                "repository-list",
                true,
                true,
                SIDEBAR_CHROME,
            )?,
            panel("agents", "agent-list", true, false, LIST_PANE_CHROME)?,
        ],
        initial_focus: PanelId::parse("repositories")?,
        focus_order: focus_order(&["repositories", "agents"])?,
        layout: LayoutNode::Split {
            axis: Axis::Horizontal,
            children: vec![
                fixed_child(leaf("repositories")?, REPOSITORY_PANEL_COLUMNS),
                collapsible_child(leaf("agents")?, weight(1), PANEL_MIN_COLUMNS, 0),
            ],
        },
    })
}

/// `core.repositories` — the split view: repositories, agents, and the
/// embedded terminal.
fn repositories_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenId::parse("core.repositories")?,
        title: "Repositories".to_owned(),
        route: RouteId::parse("repositories")?,
        panels: vec![
            panel(
                "repositories",
                "repository-list",
                true,
                true,
                SIDEBAR_CHROME,
            )?,
            panel("agents", "agent-list", true, false, LIST_PANE_CHROME)?,
            panel("terminal", PTY_PANEL_TYPE, true, true, TERMINAL_CHROME)?,
        ],
        initial_focus: PanelId::parse("repositories")?,
        focus_order: focus_order(&["repositories", "agents", "terminal"])?,
        layout: LayoutNode::Split {
            axis: Axis::Horizontal,
            children: vec![
                fixed_child(leaf("repositories")?, REPOSITORY_PANEL_COLUMNS),
                required_child(
                    LayoutNode::Split {
                        axis: Axis::Vertical,
                        children: vec![
                            collapsible_child(leaf("agents")?, weight(1), AGENT_PANEL_MIN_ROWS, 0),
                            required_child(leaf("terminal")?, weight(3), TERMINAL_PANEL_MIN_ROWS),
                        ],
                    },
                    weight(1),
                    PANEL_MIN_COLUMNS,
                ),
            ],
        },
    })
}

/// `github.issues` — issue list above the issue detail.
fn issues_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenId::parse("github.issues")?,
        title: "Issues".to_owned(),
        route: RouteId::parse("issues")?,
        panels: vec![
            panel("issue-list", "issue-list", true, true, LIST_PANE_CHROME)?,
            panel(
                "issue-detail",
                "issue-detail",
                true,
                false,
                DETAIL_PANE_CHROME,
            )?,
        ],
        initial_focus: PanelId::parse("issue-list")?,
        focus_order: focus_order(&["issue-list", "issue-detail"])?,
        layout: LayoutNode::Split {
            axis: Axis::Vertical,
            children: vec![
                required_child(leaf("issue-list")?, weight(1), LIST_PANEL_MIN_ROWS),
                collapsible_child(leaf("issue-detail")?, weight(2), DETAIL_PANEL_MIN_ROWS, 0),
            ],
        },
    })
}

/// `github.pull-requests` — PR list, detail, and the actions panel.
///
/// Collapse order follows the parity table: the detail panel is hidden before
/// the actions panel, so the lower `collapse_priority` belongs to the detail.
fn pull_requests_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenId::parse("github.pull-requests")?,
        title: "Pull Requests".to_owned(),
        route: RouteId::parse("pull-requests")?,
        panels: vec![
            panel("pr-list", "pr-list", true, true, LIST_PANE_CHROME)?,
            panel("pr-detail", "pr-detail", true, false, DETAIL_PANE_CHROME)?,
            panel("pr-actions", "pr-actions", true, false, LIST_PANE_CHROME)?,
        ],
        initial_focus: PanelId::parse("pr-list")?,
        focus_order: focus_order(&["pr-list", "pr-detail", "pr-actions"])?,
        layout: LayoutNode::Split {
            axis: Axis::Vertical,
            children: vec![
                required_child(leaf("pr-list")?, weight(1), LIST_PANEL_MIN_ROWS),
                collapsible_child(leaf("pr-detail")?, weight(2), DETAIL_PANEL_MIN_ROWS, 0),
                collapsible_child(leaf("pr-actions")?, weight(1), ACTIONS_PANEL_MIN_ROWS, 1),
            ],
        },
    })
}

/// `github.actions` — workflow-run list above the run detail.
fn actions_screen() -> Result<ScreenDescriptor, RegistryError> {
    Ok(ScreenDescriptor {
        id: ScreenId::parse("github.actions")?,
        title: "Actions".to_owned(),
        route: RouteId::parse("actions")?,
        panels: vec![
            panel("action-list", "action-list", true, true, LIST_PANE_CHROME)?,
            panel(
                "action-detail",
                "action-detail",
                true,
                false,
                DETAIL_PANE_CHROME,
            )?,
        ],
        initial_focus: PanelId::parse("action-list")?,
        focus_order: focus_order(&["action-list", "action-detail"])?,
        layout: LayoutNode::Split {
            axis: Axis::Vertical,
            children: vec![
                required_child(leaf("action-list")?, weight(1), LIST_PANEL_MIN_ROWS),
                collapsible_child(leaf("action-detail")?, weight(2), DETAIL_PANEL_MIN_ROWS, 0),
            ],
        },
    })
}
