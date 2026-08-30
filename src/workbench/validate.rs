//! Structural invariants every compiled screen descriptor satisfies
//! (issue #384, CW04-01).
//!
//! Validation is pure and total: it never panics and never mutates the
//! descriptor. A malformed descriptor is reported as a typed
//! [`DescriptorError`] naming the screen, the panel where applicable, and the
//! violated rule, so a compiled-in mistake fails at startup and in tests before
//! any renderer can observe a half-formed screen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::descriptor::{LayoutNode, ScreenDescriptor};
use super::ids::{
    IdError, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN, MAX_PORTS_PER_PANEL, MAX_SPLIT_CHILDREN,
    MIN_SPLIT_CHILDREN, VersionedTypeId,
};
use super::relationships::{RelationshipError, validate_relationships};

/// A violated descriptor invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    /// A declared identifier violates the closed identifier grammar.
    MalformedIdentifier {
        /// Offending screen.
        screen: &'static str,
        /// The offending identifier text.
        identifier: &'static str,
        /// Which rule it violated.
        reason: IdError,
    },
    /// The screen declares no panels.
    NoPanels {
        /// Offending screen.
        screen: &'static str,
    },
    /// The same host-owned overlay kind is declared more than once.
    DuplicateOverlay {
        /// Offending screen.
        screen: &'static str,
        /// Repeated closed overlay kind.
        overlay: &'static str,
    },
    /// A compiled host model was paired with the wrong closed control kind.
    HostPanelCapabilityMismatch {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel.
        panel: &'static str,
    },
    /// The screen declares more than [`MAX_PANELS_PER_SCREEN`] panels.
    TooManyPanels {
        /// Offending screen.
        screen: &'static str,
        /// Declared panel count.
        count: usize,
    },
    /// Two panels share one identity.
    DuplicatePanel {
        /// Offending screen.
        screen: &'static str,
        /// Repeated panel identity.
        panel: &'static str,
    },
    /// A panel is declared but never placed in the layout tree.
    PanelNotInLayout {
        /// Offending screen.
        screen: &'static str,
        /// Unplaced panel identity.
        panel: &'static str,
    },
    /// The layout tree places a panel the screen does not declare.
    LayoutPanelNotDeclared {
        /// Offending screen.
        screen: &'static str,
        /// Undeclared panel identity.
        panel: &'static str,
    },
    /// The layout tree places one panel more than once.
    PanelPlacedTwice {
        /// Offending screen.
        screen: &'static str,
        /// Repeated panel identity.
        panel: &'static str,
    },
    /// A focusable panel is missing from the focus order.
    FocusOrderMissingPanel {
        /// Offending screen.
        screen: &'static str,
        /// Panel absent from the focus order.
        panel: &'static str,
    },
    /// The focus order names a panel more than once.
    FocusOrderDuplicate {
        /// Offending screen.
        screen: &'static str,
        /// Repeated panel identity.
        panel: &'static str,
    },
    /// The focus order names a panel that is not focusable or not declared.
    FocusOrderUnfocusablePanel {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel identity.
        panel: &'static str,
    },
    /// The initial focus is not a focusable declared panel.
    InitialFocusNotFocusable {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel identity.
        panel: &'static str,
    },
    /// No panel is both required and focusable, so the too-small fallback
    /// would have nothing to preserve.
    NoRequiredFocusablePanel {
        /// Offending screen.
        screen: &'static str,
    },
    /// A split node declares a child count outside `[2, 8]`.
    SplitChildCount {
        /// Offending screen.
        screen: &'static str,
        /// Declared child count.
        count: usize,
    },
    /// The layout tree nests deeper than [`MAX_LAYOUT_DEPTH`].
    LayoutTooDeep {
        /// Offending screen.
        screen: &'static str,
        /// Measured depth.
        depth: usize,
    },
    /// A child declares a `max` below its `min`.
    ChildMaxBelowMin {
        /// Offending screen.
        screen: &'static str,
        /// Declared minimum.
        min: u16,
        /// Declared maximum.
        max: u16,
    },
    /// A collapsible child declares no collapse priority, so collapse order
    /// would be ambiguous.
    CollapsiblePriorityMissing {
        /// Offending screen.
        screen: &'static str,
    },
    /// A required panel sits under a collapsible child, so it could be hidden.
    RequiredPanelCollapsible {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel identity.
        panel: &'static str,
    },
    /// A panel declares more than [`MAX_PORTS_PER_PANEL`] ports.
    TooManyPorts {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel identity.
        panel: &'static str,
        /// Declared port count.
        count: usize,
    },
    /// One panel declares two ports with the same identity.
    DuplicatePort {
        /// Offending screen.
        screen: &'static str,
        /// Offending panel identity.
        panel: &'static str,
        /// Repeated port identity.
        port: &'static str,
    },
    /// The relationship graph violates one of its invariants.
    Relationship {
        /// Offending screen.
        screen: &'static str,
        /// The violated invariant.
        reason: RelationshipError,
    },
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_panel_violation(formatter)
            .or_else(|| self.fmt_focus_violation(formatter))
            .or_else(|| self.fmt_layout_violation(formatter))
            .or_else(|| self.fmt_port_violation(formatter))
            .unwrap_or(Ok(()))
    }
}

impl DescriptorError {
    /// Render the panel-set and layout-placement violations, if this is one.
    fn fmt_panel_violation(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        Some(match self {
            Self::MalformedIdentifier {
                screen,
                identifier,
                reason,
            } => write!(
                formatter,
                "screen {screen} declares identifier {identifier:?}: {reason}"
            ),
            Self::NoPanels { screen } => write!(formatter, "screen {screen} declares no panels"),
            Self::DuplicateOverlay { screen, overlay } => {
                write!(
                    formatter,
                    "screen {screen} declares overlay {overlay} twice"
                )
            }
            Self::HostPanelCapabilityMismatch { screen, panel } => write!(
                formatter,
                "screen {screen} panel {panel} pairs a host model with an incompatible control kind"
            ),
            Self::TooManyPanels { screen, count } => write!(
                formatter,
                "screen {screen} declares {count} panels (max {MAX_PANELS_PER_SCREEN})"
            ),
            Self::DuplicatePanel { screen, panel } => {
                write!(formatter, "screen {screen} declares panel {panel} twice")
            }
            Self::PanelNotInLayout { screen, panel } => write!(
                formatter,
                "screen {screen} declares panel {panel} but never places it in the layout"
            ),
            Self::LayoutPanelNotDeclared { screen, panel } => write!(
                formatter,
                "screen {screen} places undeclared panel {panel} in the layout"
            ),
            Self::PanelPlacedTwice { screen, panel } => write!(
                formatter,
                "screen {screen} places panel {panel} in the layout twice"
            ),
            _ => return None,
        })
    }

    /// Render the focus-order violations, if this is one.
    fn fmt_focus_violation(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        Some(match self {
            Self::FocusOrderMissingPanel { screen, panel } => write!(
                formatter,
                "screen {screen} omits focusable panel {panel} from the focus order"
            ),
            Self::FocusOrderDuplicate { screen, panel } => write!(
                formatter,
                "screen {screen} lists panel {panel} twice in the focus order"
            ),
            Self::FocusOrderUnfocusablePanel { screen, panel } => write!(
                formatter,
                "screen {screen} lists non-focusable or undeclared panel {panel} in the focus order"
            ),
            Self::InitialFocusNotFocusable { screen, panel } => write!(
                formatter,
                "screen {screen} sets initial focus to non-focusable panel {panel}"
            ),
            Self::NoRequiredFocusablePanel { screen } => write!(
                formatter,
                "screen {screen} has no required focusable panel to preserve when space runs out"
            ),
            _ => return None,
        })
    }

    /// Render the layout-shape violations, if this is one.
    fn fmt_layout_violation(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        Some(match self {
            Self::SplitChildCount { screen, count } => write!(
                formatter,
                "screen {screen} declares a split with {count} children (allowed {MIN_SPLIT_CHILDREN}..={MAX_SPLIT_CHILDREN})"
            ),
            Self::LayoutTooDeep { screen, depth } => write!(
                formatter,
                "screen {screen} nests {depth} levels deep (max {MAX_LAYOUT_DEPTH})"
            ),
            Self::ChildMaxBelowMin { screen, min, max } => write!(
                formatter,
                "screen {screen} declares a child with max {max} below min {min}"
            ),
            Self::CollapsiblePriorityMissing { screen } => write!(
                formatter,
                "screen {screen} declares a collapsible child without a collapse priority"
            ),
            Self::RequiredPanelCollapsible { screen, panel } => write!(
                formatter,
                "screen {screen} places required panel {panel} under a collapsible child"
            ),
            _ => return None,
        })
    }

    /// Render the port-declaration violations, if this is one.
    fn fmt_port_violation(&self, formatter: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        Some(match self {
            Self::TooManyPorts {
                screen,
                panel,
                count,
            } => write!(
                formatter,
                "screen {screen} panel {panel} declares {count} ports (max {MAX_PORTS_PER_PANEL})"
            ),
            Self::DuplicatePort {
                screen,
                panel,
                port,
            } => write!(
                formatter,
                "screen {screen} panel {panel} declares port {port} twice"
            ),
            Self::Relationship { screen, reason } => {
                write!(formatter, "screen {screen} relationship: {reason}")
            }
            _ => return None,
        })
    }
}

impl std::error::Error for DescriptorError {}

/// Check every structural invariant of one compiled screen descriptor.
///
/// # Errors
///
/// Returns the first violated invariant, naming the screen and panel involved.
pub fn validate_descriptor(descriptor: &ScreenDescriptor) -> Result<(), DescriptorError> {
    let screen = descriptor.id.as_str();
    check_identifiers(descriptor, screen)?;
    check_overlays(descriptor, screen)?;
    check_panel_set(descriptor, screen)?;
    check_host_panel_capabilities(descriptor, screen)?;
    check_ports(descriptor, screen)?;
    check_layout_placement(descriptor, screen)?;
    check_focus(descriptor, screen)?;
    check_layout_shape(&descriptor.layout, descriptor, screen, 1)?;
    validate_relationships(descriptor)
        .map_err(|reason| DescriptorError::Relationship { screen, reason })
}
fn check_overlays(
    descriptor: &ScreenDescriptor,
    screen: &'static str,
) -> Result<(), DescriptorError> {
    let mut seen = BTreeSet::new();
    for overlay in &descriptor.overlays {
        if !seen.insert(*overlay) {
            return Err(DescriptorError::DuplicateOverlay {
                screen,
                overlay: overlay.as_str(),
            });
        }
    }
    Ok(())
}
fn check_host_panel_capabilities(
    descriptor: &ScreenDescriptor,
    screen: &'static str,
) -> Result<(), DescriptorError> {
    for panel in &descriptor.panels {
        if panel
            .host_capability
            .is_some_and(|capability| !capability.is_consistent())
        {
            return Err(DescriptorError::HostPanelCapabilityMismatch {
                screen,
                panel: panel.id.as_str(),
            });
        }
    }
    Ok(())
}

/// Check that every panel declares a bounded set of distinctly named ports.
fn check_ports(descriptor: &ScreenDescriptor, screen: &'static str) -> Result<(), DescriptorError> {
    for panel in &descriptor.panels {
        if panel.ports.len() > MAX_PORTS_PER_PANEL {
            return Err(DescriptorError::TooManyPorts {
                screen,
                panel: panel.id.as_str(),
                count: panel.ports.len(),
            });
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for port in &panel.ports {
            if !seen.insert(port.id.as_str()) {
                return Err(DescriptorError::DuplicatePort {
                    screen,
                    panel: panel.id.as_str(),
                    port: port.id.as_str(),
                });
            }
        }
    }
    Ok(())
}

/// Check every identifier the descriptor declares against the closed grammar.
///
/// Identifiers are declared as constants so they can be used in patterns, which
/// means their grammar is not checked at construction. This is where that check
/// happens, and it runs before publication and in tests.
fn check_identifiers(
    descriptor: &ScreenDescriptor,
    screen: &'static str,
) -> Result<(), DescriptorError> {
    let bad = |error: IdError, identifier: &'static str| DescriptorError::MalformedIdentifier {
        screen,
        identifier,
        reason: error,
    };
    descriptor
        .id
        .check()
        .map_err(|error| bad(error, descriptor.id.as_str()))?;
    descriptor
        .route
        .check()
        .map_err(|error| bad(error, descriptor.route.as_str()))?;
    for panel in &descriptor.panels {
        panel
            .id
            .check()
            .map_err(|error| bad(error, panel.id.as_str()))?;
        panel
            .panel_type
            .check()
            .map_err(|error| bad(error, panel.panel_type.as_str()))?;
        for port in &panel.ports {
            port.id
                .check()
                .map_err(|error| bad(error, port.id.as_str()))?;
            VersionedTypeId::parse(port.type_id.as_str())
                .map_err(|error| bad(error, port.type_id.as_str()))?;
        }
    }
    Ok(())
}

fn check_panel_set(
    descriptor: &ScreenDescriptor,
    screen: &'static str,
) -> Result<(), DescriptorError> {
    if descriptor.panels.is_empty() {
        return Err(DescriptorError::NoPanels { screen });
    }
    if descriptor.panels.len() > MAX_PANELS_PER_SCREEN {
        return Err(DescriptorError::TooManyPanels {
            screen,
            count: descriptor.panels.len(),
        });
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for panel in &descriptor.panels {
        if !seen.insert(panel.id.as_str()) {
            return Err(DescriptorError::DuplicatePanel {
                screen,
                panel: panel.id.as_str(),
            });
        }
    }
    Ok(())
}

fn check_layout_placement(
    descriptor: &ScreenDescriptor,
    screen: &'static str,
) -> Result<(), DescriptorError> {
    let placed = descriptor.layout.panels_depth_first();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for panel in &placed {
        *counts.entry(panel.as_str()).or_default() += 1;
    }
    for (panel, count) in &counts {
        if *count > 1 {
            return Err(DescriptorError::PanelPlacedTwice { screen, panel });
        }
        if !descriptor
            .panels
            .iter()
            .any(|declared| declared.id.as_str() == *panel)
        {
            return Err(DescriptorError::LayoutPanelNotDeclared { screen, panel });
        }
    }
    for panel in &descriptor.panels {
        if !counts.contains_key(panel.id.as_str()) {
            return Err(DescriptorError::PanelNotInLayout {
                screen,
                panel: panel.id.as_str(),
            });
        }
    }
    Ok(())
}

fn check_focus(descriptor: &ScreenDescriptor, screen: &'static str) -> Result<(), DescriptorError> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for id in &descriptor.focus_order {
        if !seen.insert(id.as_str()) {
            return Err(DescriptorError::FocusOrderDuplicate {
                screen,
                panel: id.as_str(),
            });
        }
        if !descriptor
            .panel(id)
            .is_some_and(|declared| declared.focusable)
        {
            return Err(DescriptorError::FocusOrderUnfocusablePanel {
                screen,
                panel: id.as_str(),
            });
        }
    }
    for panel in &descriptor.panels {
        if panel.focusable && !seen.contains(panel.id.as_str()) {
            return Err(DescriptorError::FocusOrderMissingPanel {
                screen,
                panel: panel.id.as_str(),
            });
        }
    }
    if !descriptor
        .panel(&descriptor.initial_focus)
        .is_some_and(|panel| panel.focusable)
    {
        return Err(DescriptorError::InitialFocusNotFocusable {
            screen,
            panel: descriptor.initial_focus.as_str(),
        });
    }
    if descriptor.first_required_focusable().is_none() {
        return Err(DescriptorError::NoRequiredFocusablePanel { screen });
    }
    Ok(())
}

fn check_layout_shape(
    node: &LayoutNode,
    descriptor: &ScreenDescriptor,
    screen: &'static str,
    depth: usize,
) -> Result<(), DescriptorError> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(DescriptorError::LayoutTooDeep { screen, depth });
    }
    let LayoutNode::Split { children, .. } = node else {
        return Ok(());
    };
    if children.len() < MIN_SPLIT_CHILDREN || children.len() > MAX_SPLIT_CHILDREN {
        return Err(DescriptorError::SplitChildCount {
            screen,
            count: children.len(),
        });
    }
    for child in children {
        if let Some(max) = child.max
            && max < child.min
        {
            return Err(DescriptorError::ChildMaxBelowMin {
                screen,
                min: child.min,
                max,
            });
        }
        if child.collapsible {
            if child.collapse_priority.is_none() {
                return Err(DescriptorError::CollapsiblePriorityMissing { screen });
            }
            for panel in child.node.panels_depth_first() {
                if descriptor
                    .panel(panel)
                    .is_some_and(|declared| declared.required)
                {
                    return Err(DescriptorError::RequiredPanelCollapsible {
                        screen,
                        panel: panel.as_str(),
                    });
                }
            }
        }
        check_layout_shape(&child.node, descriptor, screen, depth + 1)?;
    }
    Ok(())
}
