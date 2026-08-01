//! The sole executable layout resolver (issue #384).
//!
//! [`resolve_layout`] turns a descriptor, an outer rectangle, and the
//! application's panel state into one immutable [`ResolvedLayout`]. Every
//! geometry consumer — renderer, mouse routing, selection, wrapping, scrolling,
//! and PTY resize — reads that snapshot, so no two consumers can disagree about
//! where a panel is.
//!
//! Guarantees the snapshot carries:
//!
//! - rectangles along an axis are contiguous and non-overlapping;
//! - a hidden panel has no hit, content, or PTY region at all;
//! - a visible PTY panel always has a nonzero content rectangle — when one
//!   cannot be produced the screen falls back to the too-small layout rather
//!   than emitting a zero-sized PTY;
//! - when the required panels cannot fit, exactly the first required focusable
//!   panel in descriptor focus order is visible, with a [`TooSmall`] notice.

use std::collections::BTreeSet;

use super::allocate::{AxisChild, LayoutError, allocate_axis};
use super::config::panel_insets;
use super::descriptor::{Axis, LayoutChild, LayoutNode, ScreenDescriptor};
use super::geometry::{Extent, Rect};
use super::ids::{PanelId, ScreenInstanceId};
use super::screens::PTY_PANEL_TYPE;

/// Panels the application has hidden for reasons the descriptor does not model
/// (no selection, a closed detail pane, an unavailable data source).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelState {
    hidden: BTreeSet<String>,
}

impl PanelState {
    /// A state in which every panel is shown.
    #[must_use]
    pub fn all_visible() -> Self {
        Self::default()
    }

    /// Hide one panel.
    #[must_use]
    pub fn hiding(mut self, panel: &PanelId) -> Self {
        self.hidden.insert(panel.as_str().to_owned());
        self
    }

    /// Whether the application has hidden this panel.
    #[must_use]
    pub fn is_hidden(&self, panel: &PanelId) -> bool {
        self.hidden.contains(panel.as_str())
    }
}

/// The space a screen needed against the space it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooSmall {
    /// Smallest extent in which the required panels fit.
    pub needed: Extent,
    /// Extent the screen was actually given.
    pub available: Extent,
}

/// One panel's resolved geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPanel {
    /// Which panel this is.
    pub id: PanelId,
    /// Whether the panel occupies cells this frame.
    pub visible: bool,
    /// The panel's whole rectangle, including its border and title.
    pub chrome: Rect,
    /// The rectangle inside the panel's border and title.
    pub content: Rect,
    /// Position of the panel in the layout tree's depth-first order.
    pub depth_first_index: usize,
    /// Rectangle that accepts mouse events, or `None` when hidden.
    pub hit_region: Option<Rect>,
}

/// One screen's resolved geometry for one size and state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLayout {
    /// Identity of the snapshot. Consumers compare this to prove they read the
    /// same geometry the renderer used.
    pub screen_instance: ScreenInstanceId,
    /// The rectangle the screen was resolved into, after global chrome.
    pub outer: Rect,
    /// Every declared panel, in descriptor declaration order.
    pub panels: Vec<ResolvedPanel>,
    /// Set when the required panels did not fit.
    pub too_small: Option<TooSmall>,
}

impl ResolvedLayout {
    /// Look up one panel's resolved geometry.
    #[must_use]
    pub fn panel(&self, id: &PanelId) -> Option<&ResolvedPanel> {
        self.panels.iter().find(|panel| &panel.id == id)
    }

    /// Every visible panel, in declaration order.
    pub fn visible_panels(&self) -> impl Iterator<Item = &ResolvedPanel> {
        self.panels.iter().filter(|panel| panel.visible)
    }

    /// The panel whose hit region contains a cell, innermost first.
    ///
    /// Regions never overlap, so at most one panel matches.
    #[must_use]
    pub fn panel_at(&self, col: u16, row: u16) -> Option<&ResolvedPanel> {
        self.panels.iter().find(|panel| {
            panel
                .hit_region
                .is_some_and(|region| region.contains(col, row))
        })
    }
}

/// Resolve one screen's geometry.
///
/// # Errors
///
/// Returns [`LayoutError::Overflow`] if interior arithmetic leaves the checked
/// range. The resolver never panics.
pub fn resolve_layout(
    descriptor: &ScreenDescriptor,
    screen_instance: ScreenInstanceId,
    outer: Rect,
    state: &PanelState,
) -> Result<ResolvedLayout, LayoutError> {
    let mut placements: Vec<Placement> = Vec::new();
    let mut cursor = 0_usize;
    index_panels(&descriptor.layout, &mut cursor, &mut placements);

    let outcome = place(&descriptor.layout, outer, state, &mut placements)?;
    let mut layout = build_layout(descriptor, screen_instance, outer, &placements);

    let unfit = match outcome {
        Fit::Fits => hide_degenerate_panels(descriptor, &mut layout),
        Fit::Unfit(needed) => Some(needed),
    };

    if let Some(needed) = unfit {
        return Ok(too_small_layout(descriptor, screen_instance, outer, needed));
    }
    Ok(layout)
}

/// Repair a prior focus against a resolved snapshot.
///
/// Advances cyclically from the prior focus to the first visible focusable
/// panel at or after it, then falls back to the descriptor's initial focus, and
/// finally to any visible focusable panel. Returns `None` only when no
/// focusable panel is visible at all.
#[must_use]
pub fn repair_focus(
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
    prior: Option<&PanelId>,
) -> Option<PanelId> {
    let order = &descriptor.focus_order;
    if order.is_empty() {
        return None;
    }
    let is_visible = |id: &PanelId| layout.panel(id).is_some_and(|resolved| resolved.visible);

    // With no prior focus the screen opens on its declared initial focus, which
    // is not necessarily the head of the focus order: the workspace screens
    // cycle through the repository sidebar first but open on their list.
    let anchor = prior.unwrap_or(&descriptor.initial_focus);
    let start = order
        .iter()
        .position(|candidate| candidate == anchor)
        .unwrap_or(0);
    for offset in 0..order.len() {
        let index = (start + offset) % order.len();
        if let Some(candidate) = order.get(index)
            && is_visible(candidate)
        {
            return Some(*candidate);
        }
    }
    None
}

/// The rectangle a PTY panel should be resized to, if it is showing.
///
/// This is the single call the terminal view and the PTY resize path make.
/// It returns `None` when the panel is hidden or is not a PTY panel, and never
/// returns a zero-area rectangle, so no caller needs its own `.max(1)` guard.
#[must_use]
pub fn pty_content_rect(
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
    panel: &PanelId,
) -> Option<Rect> {
    let declared = descriptor.panel(panel)?;
    if declared.panel_type.as_str() != PTY_PANEL_TYPE {
        return None;
    }
    let resolved = layout.panel(panel)?;
    if !resolved.visible || resolved.content.is_empty() {
        return None;
    }
    Some(resolved.content)
}

/// Whether a subtree fit the rectangle it was given.
enum Fit {
    Fits,
    Unfit(Extent),
}

/// A panel's position in the tree plus the rectangle it was placed in.
struct Placement {
    panel: PanelId,
    depth_first_index: usize,
    rect: Option<Rect>,
}

fn index_panels(node: &LayoutNode, cursor: &mut usize, placements: &mut Vec<Placement>) {
    match node {
        LayoutNode::Leaf { panel } => {
            placements.push(Placement {
                panel: *panel,
                depth_first_index: *cursor,
                rect: None,
            });
            *cursor += 1;
        }
        LayoutNode::Split { children, .. } => {
            for child in children {
                index_panels(&child.node, cursor, placements);
            }
        }
    }
}

fn place(
    node: &LayoutNode,
    rect: Rect,
    state: &PanelState,
    placements: &mut [Placement],
) -> Result<Fit, LayoutError> {
    match node {
        LayoutNode::Leaf { panel } => {
            if let Some(slot) = placements
                .iter_mut()
                .find(|placement| &placement.panel == panel)
            {
                slot.rect = Some(rect);
            }
            Ok(Fit::Fits)
        }
        LayoutNode::Split {
            axis,
            gap,
            children,
        } => place_split(*axis, *gap, children, rect, state, placements),
    }
}

fn place_split(
    axis: Axis,
    gap: u16,
    children: &[LayoutChild],
    rect: Rect,
    state: &PanelState,
    placements: &mut [Placement],
) -> Result<Fit, LayoutError> {
    let axis_children: Vec<AxisChild> = children
        .iter()
        .map(|child| axis_child(child, state, placements))
        .collect();
    let available = match axis {
        Axis::Horizontal => rect.width,
        Axis::Vertical => rect.height,
    };
    let allocation = allocate_axis(&axis_children, available, gap)?;
    if !allocation.fits {
        let needed = u16::try_from(allocation.needed).unwrap_or(u16::MAX);
        return Ok(Fit::Unfit(match axis {
            Axis::Horizontal => Extent::new(needed, rect.height),
            Axis::Vertical => Extent::new(rect.width, needed),
        }));
    }

    let mut offset = 0_u16;
    let mut first_visible = true;
    let mut worst: Option<Extent> = None;
    for (child, cells) in children.iter().zip(&allocation.cells) {
        let Some(cells) = *cells else {
            continue;
        };
        if !first_visible {
            offset = offset.saturating_add(gap);
        }
        first_visible = false;
        let child_rect = match axis {
            Axis::Horizontal => Rect::new(
                rect.col.saturating_add(offset),
                rect.row,
                cells,
                rect.height,
            ),
            Axis::Vertical => {
                Rect::new(rect.col, rect.row.saturating_add(offset), rect.width, cells)
            }
        };
        offset = offset.saturating_add(cells);
        if let Fit::Unfit(needed) = place(&child.node, child_rect, state, placements)? {
            worst = Some(worst.map_or(needed, |current| {
                Extent::new(current.cols.max(needed.cols), current.rows.max(needed.rows))
            }));
        }
    }
    Ok(worst.map_or(Fit::Fits, Fit::Unfit))
}

/// Reduce a layout child to the values one-axis allocation needs.
///
/// A child is treated as application-hidden only when *every* panel beneath it
/// is hidden, so hiding one leaf of a split does not erase its siblings.
fn axis_child(child: &LayoutChild, state: &PanelState, placements: &[Placement]) -> AxisChild {
    let panels = child.node.panels_depth_first();
    let hidden = !panels.is_empty() && panels.iter().all(|panel| state.is_hidden(panel));
    let depth_first_index = panels
        .first()
        .and_then(|panel| {
            placements
                .iter()
                .find(|placement| &&placement.panel == panel)
                .map(|placement| placement.depth_first_index)
        })
        .unwrap_or(0);
    AxisChild {
        size: child.size,
        min: child.min,
        max: child.max,
        collapsible: child.collapsible,
        collapse_priority: child.collapse_priority.unwrap_or(0),
        depth_first_index,
        hidden,
    }
}

fn build_layout(
    descriptor: &ScreenDescriptor,
    screen_instance: ScreenInstanceId,
    outer: Rect,
    placements: &[Placement],
) -> ResolvedLayout {
    let panels = descriptor
        .panels
        .iter()
        .map(|panel| {
            let placement = placements
                .iter()
                .find(|placement| placement.panel == panel.id);
            let depth_first_index = placement.map_or(0, |found| found.depth_first_index);
            let rect = placement.and_then(|found| found.rect);
            resolved_panel(
                panel.id,
                rect,
                depth_first_index,
                panel_insets(&panel.config),
            )
        })
        .collect();
    ResolvedLayout {
        screen_instance,
        outer,
        panels,
        too_small: None,
    }
}

fn resolved_panel(
    id: PanelId,
    rect: Option<Rect>,
    depth_first_index: usize,
    insets: super::geometry::Insets,
) -> ResolvedPanel {
    match rect.filter(|found| !found.is_empty()) {
        Some(chrome) => ResolvedPanel {
            id,
            visible: true,
            chrome,
            content: chrome.inset(insets),
            depth_first_index,
            hit_region: Some(chrome),
        },
        None => hidden_panel(id, depth_first_index),
    }
}

fn hidden_panel(id: PanelId, depth_first_index: usize) -> ResolvedPanel {
    ResolvedPanel {
        id,
        visible: false,
        chrome: Rect::default(),
        content: Rect::default(),
        depth_first_index,
        hit_region: None,
    }
}

/// Hide panels whose content collapsed to nothing.
///
/// A visible panel with no content rows or columns would render as an empty
/// husk and, for a PTY panel, would resize a live terminal to zero. When such a
/// panel is required the whole screen is too small; when it is optional it is
/// simply hidden.
fn hide_degenerate_panels(
    descriptor: &ScreenDescriptor,
    layout: &mut ResolvedLayout,
) -> Option<Extent> {
    let mut required_failure: Option<Extent> = None;
    for resolved in &mut layout.panels {
        if !resolved.visible {
            continue;
        }
        let Some(declared) = descriptor.panel(&resolved.id) else {
            continue;
        };
        // A PTY panel is called out explicitly because a zero content rect
        // there would resize a live terminal to nothing, not merely render an
        // empty box.
        let degenerate = resolved.content.is_empty();
        if !degenerate {
            continue;
        }
        if declared.required {
            let insets = panel_insets(&declared.config);
            let needed = Extent::new(
                u16::try_from(insets.horizontal().saturating_add(1)).unwrap_or(u16::MAX),
                u16::try_from(insets.vertical().saturating_add(1)).unwrap_or(u16::MAX),
            );
            required_failure = Some(required_failure.map_or(needed, |current| {
                Extent::new(current.cols.max(needed.cols), current.rows.max(needed.rows))
            }));
            continue;
        }
        *resolved = hidden_panel(resolved.id, resolved.depth_first_index);
    }
    required_failure
}

/// Build the fallback shown when the required panels do not fit: exactly the
/// first required focusable panel, over the whole rectangle.
fn too_small_layout(
    descriptor: &ScreenDescriptor,
    screen_instance: ScreenInstanceId,
    outer: Rect,
    needed: Extent,
) -> ResolvedLayout {
    let survivor = descriptor.first_required_focusable().map(|panel| &panel.id);
    let panels = descriptor
        .panels
        .iter()
        .enumerate()
        .map(|(index, panel)| {
            if Some(&panel.id) == survivor && !outer.is_empty() {
                // The survivor still draws its own border and title inside the
                // rectangle, so its content area is inset exactly as it would
                // be on the normal path. Reporting the whole rectangle as
                // content would tell a PTY consumer it has more cells than it
                // can actually draw in.
                let content = outer.inset(panel_insets(&panel.config));
                ResolvedPanel {
                    id: panel.id,
                    visible: true,
                    chrome: outer,
                    content,
                    depth_first_index: index,
                    hit_region: Some(outer),
                }
            } else {
                hidden_panel(panel.id, index)
            }
        })
        .collect();
    ResolvedLayout {
        screen_instance,
        outer,
        panels,
        too_small: Some(TooSmall {
            needed,
            available: outer.extent(),
        }),
    }
}
