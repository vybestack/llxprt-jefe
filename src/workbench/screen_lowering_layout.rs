//! Lowering the layout tree and the relationship list (issue #385, CW05-02).
//!
//! Both are plain structural copies. The one substantive decision is the split
//! gap: the external syntax does not expose it, and every definable panel draws
//! its own border and title inside its own rectangle, so a definition's splits
//! declare no divider. That is a property of the panel types a definition may
//! name, not a default the lowerer chose on the author's behalf.

use std::num::NonZeroU16;

use toml::Spanned;

use super::descriptor::{Axis, LayoutChild, LayoutNode, Size};
use super::lowering_error::LoweringError;
use super::relationships::{
    ActivationMode, EmptyPolicy, Relationship, RelationshipKind, SessionEmptyPolicy,
};
use super::screen_file::{
    ActivationModeFile, AxisFile, ChildFile, EmptyPolicyFile, LayoutFile, RelationshipFile,
    SessionEmptyPolicyFile, SizeFile,
};
use super::screen_lowering::lower_port_ref;

/// Cells between adjacent visible children of a definition's split.
///
/// Zero, because every definable panel draws its own chrome.
const DEFINITION_SPLIT_GAP: u16 = 0;

/// Lower one layout tree.
///
/// # Errors
///
/// Returns an identifier error when a leaf names a malformed panel.
pub fn lower_layout(node: &LayoutFile) -> Result<LayoutNode, LoweringError> {
    match node {
        LayoutFile::Leaf { panel } => Ok(LayoutNode::Leaf {
            panel: super::screen_lowering::lower_panel_ref(panel)?,
        }),
        LayoutFile::Split { axis, children } => Ok(LayoutNode::Split {
            axis: match axis {
                AxisFile::Horizontal => Axis::Horizontal,
                AxisFile::Vertical => Axis::Vertical,
            },
            gap: DEFINITION_SPLIT_GAP,
            children: children
                .iter()
                .map(lower_child)
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn lower_child(child: &ChildFile) -> Result<LayoutChild, LoweringError> {
    Ok(LayoutChild {
        node: lower_layout(&child.node)?,
        size: match child.size {
            // Zero is rejected during parsing, so the fallback is unreachable;
            // it exists because `NonZeroU16` has no infallible conversion and a
            // panic here would be a worse answer than the smallest size.
            SizeFile::Fixed(cells) => {
                Size::Fixed(NonZeroU16::new(cells).unwrap_or(NonZeroU16::MIN))
            }
            SizeFile::Weight(share) => {
                Size::Weight(NonZeroU16::new(share).unwrap_or(NonZeroU16::MIN))
            }
        },
        min: child.min,
        max: child.max,
        collapsible: child.collapsible,
        collapse_priority: child.collapse_priority,
    })
}

/// Lower the relationship list, preserving declaration order.
///
/// # Errors
///
/// Returns the first malformed port reference.
pub fn lower_relationships(
    declared: &[Spanned<RelationshipFile>],
) -> Result<Vec<Relationship>, LoweringError> {
    declared
        .iter()
        .map(|relationship| lower_relationship(relationship.get_ref()))
        .collect()
}

fn lower_relationship(declared: &RelationshipFile) -> Result<Relationship, LoweringError> {
    let (kind, source, target) = match declared {
        RelationshipFile::Scope { source, target } => (RelationshipKind::Scope, source, target),
        RelationshipFile::MasterDetail {
            source,
            target,
            activation,
            empty,
        } => (
            RelationshipKind::MasterDetail {
                activation: match activation {
                    ActivationModeFile::Immediate => ActivationMode::Immediate,
                    ActivationModeFile::Explicit => ActivationMode::Explicit,
                },
                empty: match empty {
                    EmptyPolicyFile::ShowNone => EmptyPolicy::ShowNone,
                    EmptyPolicyFile::ShowAll => EmptyPolicy::ShowAll,
                    EmptyPolicyFile::Retain => EmptyPolicy::Retain,
                },
            },
            source,
            target,
        ),
        RelationshipFile::SessionTarget {
            source,
            target,
            empty,
        } => (
            RelationshipKind::SessionTarget {
                empty: match empty {
                    SessionEmptyPolicyFile::Detach => SessionEmptyPolicy::Detach,
                    SessionEmptyPolicyFile::Retain => SessionEmptyPolicy::Retain,
                },
            },
            source,
            target,
        ),
    };
    Ok(Relationship {
        kind,
        source: lower_port_ref(source)?,
        target: lower_port_ref(target)?,
    })
}
