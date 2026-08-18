//! Closed descriptor and layout-tree value types (issue #384).
//!
//! These types are purely structural: they describe what a screen contains and
//! how its cells divide, and nothing else. They hold no application state, no
//! terminal handle, no theme, and no persistence coupling, which is what lets
//! the resolver be exercised exhaustively as a pure function.
//!
//! Sizing uses `NonZeroU16` so "a fixed size of zero" and "a weight of zero"
//! are unrepresentable rather than special-cased at every allocation step.

use std::fmt;
use std::num::NonZeroU16;

use crate::domain::{Id, TypedMap};

use super::activation::{ActivationField, ScreenBinding};
use super::ids::{PanelId, PanelTypeId, PortId, RouteId, ScreenIdentity, VersionedTypeId};
use super::relationships::Relationship;

/// Axis along which a split node divides its rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// Children are placed left to right; the split divides columns.
    Horizontal,
    /// Children are placed top to bottom; the split divides rows.
    Vertical,
}

/// How a child claims cells along its parent's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Size {
    /// Claim exactly this many cells, clamped to the child's `[min, max]`.
    Fixed(NonZeroU16),
    /// Claim a share of the cells left after every minimum is satisfied.
    Weight(NonZeroU16),
}

/// One child of a split node, with its allocation and collapse policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutChild {
    /// The subtree this child occupies.
    pub node: LayoutNode,
    /// How the child claims cells along the parent axis.
    pub size: Size,
    /// Fewest cells the child can occupy while remaining visible.
    pub min: u16,
    /// Most cells the child may occupy, if it is bounded.
    pub max: Option<u16>,
    /// Whether the resolver may hide this child to fit the remaining children.
    pub collapsible: bool,
    /// Collapse order key; lower values are hidden first. Only meaningful when
    /// `collapsible` is set.
    pub collapse_priority: Option<i32>,
}

/// A layout tree node: either one panel, or a split of further nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutNode {
    /// A single panel occupying the node's rectangle.
    Leaf {
        /// The panel rendered in this rectangle.
        panel: PanelId,
    },
    /// A division of the node's rectangle among ordered children.
    Split {
        /// Axis the rectangle is divided along.
        axis: Axis,
        /// Cells left between each adjacent pair of visible children.
        ///
        /// Panels draw their border and title *inside* their own rectangle, so
        /// a split of bordered panes declares zero. A nonzero gap is for
        /// splits that want a drawn or blank divider.
        gap: u16,
        /// Children in declaration order; allocation and remainder
        /// distribution both follow this order.
        children: Vec<LayoutChild>,
    },
}

impl LayoutNode {
    /// Collect every panel referenced by this subtree in depth-first
    /// declaration order.
    #[must_use]
    pub fn panels_depth_first(&self) -> Vec<&PanelId> {
        let mut collected = Vec::new();
        self.collect_panels(&mut collected);
        collected
    }

    fn collect_panels<'node>(&'node self, collected: &mut Vec<&'node PanelId>) {
        match self {
            Self::Leaf { panel } => collected.push(panel),
            Self::Split { children, .. } => {
                for child in children {
                    child.node.collect_panels(collected);
                }
            }
        }
    }

    /// Depth of the deepest node in this subtree, counting this node as 1.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { children, .. } => {
                1 + children
                    .iter()
                    .map(|child| child.node.depth())
                    .max()
                    .unwrap_or(0)
            }
        }
    }
}

/// Which way a value crosses a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PortDirection {
    /// The panel consumes a value here.
    Input,
    /// The panel publishes a value here.
    Output,
}

impl PortDirection {
    /// The stable text used in diagnostics and the external syntax.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

/// One typed connection point on a panel.
///
/// Ports are the only surface a relationship may join, which is what keeps
/// panels from reaching into each other: a panel declares what it publishes and
/// what it consumes, and the screen declares which publications feed which
/// consumptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDescriptor {
    /// Identity of the port within its panel.
    pub id: PortId,
    /// Immutable owner of the resource schema this port names.
    pub owner_id: Id,
    /// Which way values cross this port.
    pub direction: PortDirection,
    /// Identity and version of the value the port carries.
    pub type_id: VersionedTypeId,
    /// Whether the panel needs a value here to function.
    pub required: bool,
    /// Whether the port keeps its last value when its source becomes absent,
    /// instead of clearing.
    pub retained: bool,
}

/// A reference to one port on one panel of the same screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortRef {
    /// The panel that owns the port.
    pub panel: PanelId,
    /// The port on that panel.
    pub port: PortId,
}

impl fmt::Display for PortRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.panel, self.port)
    }
}

/// One panel within a screen: its identity, kind, configuration, and role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDescriptor {
    /// Identity of the panel within its screen.
    pub id: PanelId,
    /// Kind of content the panel renders.
    pub panel_type: PanelTypeId,
    /// Panel-specific configuration values.
    pub config: TypedMap,
    /// Whether the panel participates in the focus cycle.
    pub focusable: bool,
    /// Whether the panel must remain visible for the screen to be usable.
    /// Required panels are never collapsed; when they cannot fit the resolver
    /// falls back to the too-small layout.
    pub required: bool,
    /// Typed connection points, in declaration order.
    pub ports: Vec<PortDescriptor>,
}

impl PanelDescriptor {
    /// Find a port by identity.
    #[must_use]
    pub fn port(&self, id: &PortId) -> Option<&PortDescriptor> {
        self.ports.iter().find(|port| &port.id == id)
    }
}

/// The sole definition of one screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDescriptor {
    /// Stable identity of the screen, compiled or lowered.
    pub id: ScreenIdentity,
    /// Human-readable screen title.
    pub title: String,
    /// Navigation route the screen is reachable through.
    pub route: RouteId,
    /// Every panel in the screen. Each appears exactly once here and exactly
    /// once in `layout`.
    pub panels: Vec<PanelDescriptor>,
    /// Panel focused when the screen is first instantiated.
    pub initial_focus: PanelId,
    /// Focus cycle order; every focusable panel appears exactly once.
    pub focus_order: Vec<PanelId>,
    /// Root of the layout tree.
    pub layout: LayoutNode,
    /// Typed edges between this screen's ports, in declaration order.
    ///
    /// Declaration order is part of the contract: propagation applies edges in
    /// this order within one transition, so two screens with the same edges in
    /// different orders are two different screens.
    pub relationships: Vec<Relationship>,
    /// What this screen's route accepts when something navigates to it.
    ///
    /// Compiled screens declare none today; navigation validates an activation
    /// against this schema, so it lives beside the route rather than beside the
    /// syntax that happened to describe it.
    pub activation: Vec<ActivationField>,
    /// Actions this screen asks to be reachable while it is focused.
    pub bindings: Vec<ScreenBinding>,
}

impl ScreenDescriptor {
    /// Find a panel by identity.
    #[must_use]
    pub fn panel(&self, id: &PanelId) -> Option<&PanelDescriptor> {
        self.panels.iter().find(|panel| &panel.id == id)
    }

    /// Resolve a `<panel>.<port>` reference against this screen.
    ///
    /// Returns `None` when either half names something the screen does not
    /// declare, which is how a relationship referring outside its own screen is
    /// detected.
    #[must_use]
    pub fn port(&self, reference: &PortRef) -> Option<&PortDescriptor> {
        self.panel(&reference.panel)?.port(&reference.port)
    }

    /// The first required focusable panel in focus order.
    ///
    /// This is the panel the too-small fallback preserves, so the screen is
    /// never reduced to an empty or unfocusable state.
    #[must_use]
    pub fn first_required_focusable(&self) -> Option<&PanelDescriptor> {
        self.focus_order
            .iter()
            .filter_map(|id| self.panel(id))
            .find(|panel| panel.required && panel.focusable)
    }
}
