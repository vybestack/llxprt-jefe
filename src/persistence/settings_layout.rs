//! Rendering one layout tree as the settings document's inline syntax
//! (issue #388).
//!
//! A layout override is written in the same grammar a screen definition file
//! declares its layout in, so one shape of layout syntax is readable by anyone
//! who has read either file. It is rendered by hand rather than by a serializer
//! because a layout override is written into an existing document: the exact
//! bytes are part of the contract the editor's goldens pin, and a serializer
//! that reordered keys or promoted a nested table into its own header would
//! change syntax the user did not edit.
//!
//! Nothing here validates. The descriptor/layout validator decides whether a
//! tree is usable; this only writes down the tree it is given.

use crate::workbench::descriptor::{Axis, LayoutChild, LayoutNode, Size};

/// Render one complete layout tree as a single inline TOML value.
#[must_use]
pub(super) fn render(node: &LayoutNode) -> Vec<u8> {
    let mut out = String::new();
    write_node(&mut out, node);
    out.into_bytes()
}

fn write_node(out: &mut String, node: &LayoutNode) {
    match node {
        LayoutNode::Leaf { panel } => {
            out.push_str("{ type = \"leaf\", panel = ");
            write_string(out, panel.as_str());
            out.push_str(" }");
        }
        LayoutNode::Split { axis, children, .. } => {
            out.push_str("{ type = \"split\", axis = ");
            out.push_str(match axis {
                Axis::Horizontal => "\"horizontal\"",
                Axis::Vertical => "\"vertical\"",
            });
            out.push_str(", children = [");
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_child(out, child);
            }
            out.push_str("] }");
        }
    }
}

/// Render one child.
///
/// `max` and `collapse-priority` are written only when they are set. An absent
/// bound and a bound of zero are different statements about the child, and the
/// grammar spells the absent one by leaving the key out.
fn write_child(out: &mut String, child: &LayoutChild) {
    out.push_str("{ node = ");
    write_node(out, &child.node);
    out.push_str(", size = ");
    match child.size {
        Size::Fixed(cells) => out.push_str(&format!("{{ fixed = {cells} }}")),
        Size::Weight(share) => out.push_str(&format!("{{ weight = {share} }}")),
    }
    out.push_str(&format!(", min = {}", child.min));
    if let Some(max) = child.max {
        out.push_str(&format!(", max = {max}"));
    }
    out.push_str(&format!(", collapsible = {}", child.collapsible));
    if let Some(priority) = child.collapse_priority {
        out.push_str(&format!(", collapse-priority = {priority}"));
    }
    out.push_str(" }");
}

/// Write one TOML string, escaped exactly as the parser will read it back.
fn write_string(out: &mut String, value: &str) {
    out.push_str(&toml::Value::String(value.to_owned()).to_string());
}
