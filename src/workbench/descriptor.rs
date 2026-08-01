//! Closed descriptor and layout-tree value types (issue #384).
//!
//! These types are purely structural: they describe what a screen contains and
//! how its cells divide, and nothing else. They hold no application state, no
//! terminal handle, no theme, and no persistence coupling, which is what lets
//! the resolver be exercised exhaustively as a pure function.
//!
//! Sizing uses `NonZeroU16` so "a fixed size of zero" and "a weight of zero"
//! are unrepresentable rather than special-cased at every allocation step.

use std::num::NonZeroU16;

use crate::domain::TypedMap;

use super::ids::{PanelId, PanelTypeId, RouteId, ScreenId};

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
}

/// The sole definition of one screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDescriptor {
    /// Stable identity of the screen.
    pub id: ScreenId,
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
}

impl ScreenDescriptor {
    /// Find a panel by identity.
    #[must_use]
    pub fn panel(&self, id: &PanelId) -> Option<&PanelDescriptor> {
        self.panels.iter().find(|panel| &panel.id == id)
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
