//! Lowering the layout tree and the relationship list (issue #385, CW05-02).
//!
//! Both are plain structural copies. The one substantive decision is the split
//! gap: the external syntax does not expose it, and every definable panel draws
//! its own border and title inside its own rectangle, so a definition's splits
//! declare no divider. That is a property of the panel types a definition may
//! name, not a default the lowerer chose on the author's behalf.

use std::num::NonZeroU16;

use toml::Spanned;

use crate::domain::{TypedMap, TypedValue};

use super::descriptor::{Axis, LayoutChild, LayoutNode, Size};
use super::ids::{MAX_LAYOUT_DEPTH, MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN};
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
        // Parsing already rejects a zero extent. Rejecting it again rather than
        // coercing it to one keeps that the only answer: silently correcting a
        // size here would turn a parser regression into a layout that does not
        // match the file and cannot be traced back to it.
        size: match child.size {
            SizeFile::Fixed(cells) => Size::Fixed(nonzero(cells, "size.fixed")?),
            SizeFile::Weight(share) => Size::Weight(nonzero(share, "size.weight")?),
        },
        min: child.min,
        max: child.max,
        collapsible: child.collapsible,
        collapse_priority: child.collapse_priority,
    })
}

/// Reject a zero extent rather than correcting it.
fn nonzero(value: u16, field: &'static str) -> Result<NonZeroU16, LoweringError> {
    NonZeroU16::new(value).ok_or(LoweringError::ZeroExtent { field })
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

/// Read one settings layout override through the definition-file grammar.
///
/// A layout override in the settings document is the same declaration a screen
/// definition file makes, so it is read by the same closed grammar: unknown
/// fields are refused, `size` names exactly one of `fixed` and `weight`, and
/// the depth and child-count bounds are the ones every definition obeys. The
/// alternative — a second reader in the editor — would let the editor approve
/// syntax this owner rejects, which is how two grammars for one thing start.
///
/// # Errors
///
/// Returns the grammar's own refusal, already redacted to identifiers and
/// rule names.
pub fn lower_settings_layout(values: &TypedMap) -> Result<LayoutNode, String> {
    let value = typed_map_to_toml(values)?;
    let declared: LayoutFile = value
        .try_into()
        .map_err(|error: toml::de::Error| error.message().to_owned())?;
    check_layout_bounds(&declared)?;
    lower_layout(&declared).map_err(|error| error.to_string())
}

/// Check the bounds every declared layout obeys, before lowering it.
fn check_layout_bounds(declared: &LayoutFile) -> Result<(), String> {
    check_declared_layout(declared, 1)
}

fn check_declared_layout(node: &LayoutFile, depth: usize) -> Result<(), String> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(format!("layout nests past {MAX_LAYOUT_DEPTH} levels"));
    }
    let LayoutFile::Split { children, .. } = node else {
        return Ok(());
    };
    if children.len() < MIN_SPLIT_CHILDREN || children.len() > MAX_SPLIT_CHILDREN {
        return Err(format!(
            "a split declares {} children (allowed {MIN_SPLIT_CHILDREN}..={MAX_SPLIT_CHILDREN})",
            children.len()
        ));
    }
    for child in children {
        check_declared_layout(&child.node, depth + 1)?;
    }
    Ok(())
}

/// Render one typed configuration subtree as the TOML value it was read from.
///
/// The settings publisher hands out typed values, and the definition grammar
/// deserializes from TOML, so this is the one hop between them. A secret
/// reference has no place in a layout and is refused rather than rendered.
fn typed_map_to_toml(values: &TypedMap) -> Result<toml::Value, String> {
    let mut table = toml::map::Map::new();
    for (key, value) in values {
        table.insert(key.as_str().to_owned(), typed_value_to_toml(value)?);
    }
    Ok(toml::Value::Table(table))
}

fn typed_value_to_toml(value: &TypedValue) -> Result<toml::Value, String> {
    Ok(match value {
        TypedValue::String(text) => toml::Value::String(text.clone()),
        TypedValue::Bool(flag) => toml::Value::Boolean(*flag),
        TypedValue::Integer(number) => toml::Value::Integer(*number),
        // A layout declares cell counts and order keys, never fractions.
        // Rendering one as a string would fail deep inside the grammar with a
        // message about the wrong thing.
        TypedValue::Decimal(_) => {
            return Err("a layout declares whole numbers, not decimals".to_owned());
        }
        TypedValue::Datetime(stamp) => toml::Value::String(stamp.to_string()),
        TypedValue::List(values) => toml::Value::Array(
            values
                .iter()
                .map(typed_value_to_toml)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        TypedValue::Map(values) => typed_map_to_toml(values)?,
        TypedValue::SecretRef(_) => {
            return Err("a layout declares no secret".to_owned());
        }
    })
}
