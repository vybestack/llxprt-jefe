//! Reading one layout override back out of a published settings document
//! (issue #388).
//!
//! The document holds a layout as typed values; a descriptor holds it as a
//! [`LayoutNode`]. This is the one place that turns the first into the second,
//! so the syntax the writer emits and the syntax the editor understands cannot
//! drift apart.
//!
//! A leaf may only name a panel the screen already declares. That is not a
//! restriction the grammar invents: a panel identity is a compiled or interned
//! `'static` string, and a screen's own panels are the only ones an override
//! could sensibly rearrange. Naming anything else is reported rather than
//! interned, which is also what stops a document from growing the identifier
//! table.
//!
//! Nothing here decides whether the resulting tree is usable. That is
//! [`crate::workbench::validate::validate_descriptor`]'s answer.

use crate::domain::{Id, TypedMap, TypedValue};
use crate::workbench::descriptor::{Axis, LayoutChild, LayoutNode, ScreenDescriptor, Size};
use crate::workbench::ids::{MAX_LAYOUT_DEPTH, PanelId};

/// Read one complete layout override.
///
/// # Errors
///
/// Returns the first violated rule, in the same voice a descriptor refusal
/// uses, with no value from the document beyond the identifier it names.
pub fn read(values: &TypedMap, screen: &ScreenDescriptor) -> Result<LayoutNode, String> {
    read_node(values, screen, 1)
}

fn read_node(
    values: &TypedMap,
    screen: &ScreenDescriptor,
    depth: usize,
) -> Result<LayoutNode, String> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(format!("layout nests past {MAX_LAYOUT_DEPTH} levels"));
    }
    match string(values, "type")? {
        "leaf" => Ok(LayoutNode::Leaf {
            panel: panel(screen, string(values, "panel")?)?,
        }),
        "split" => read_split(values, screen, depth),
        other => Err(format!(
            "layout node type {other:?} is neither leaf nor split"
        )),
    }
}

fn read_split(
    values: &TypedMap,
    screen: &ScreenDescriptor,
    depth: usize,
) -> Result<LayoutNode, String> {
    let axis = match string(values, "axis")? {
        "horizontal" => Axis::Horizontal,
        "vertical" => Axis::Vertical,
        other => {
            return Err(format!(
                "split axis {other:?} is neither horizontal nor vertical"
            ));
        }
    };
    let TypedValue::List(declared) = field(values, "children")? else {
        return Err("split children must be a list".to_owned());
    };
    let children = declared
        .iter()
        .map(|child| read_child(child, screen, depth))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LayoutNode::Split {
        axis,
        // Every panel a definable layout can name draws its own border and
        // title inside its own rectangle, so an override declares no divider —
        // the same answer screen definition files get.
        gap: 0,
        children,
    })
}

fn read_child(
    value: &TypedValue,
    screen: &ScreenDescriptor,
    depth: usize,
) -> Result<LayoutChild, String> {
    let TypedValue::Map(child) = value else {
        return Err("each split child must be a table".to_owned());
    };
    let TypedValue::Map(node) = field(child, "node")? else {
        return Err("a split child's node must be a table".to_owned());
    };
    Ok(LayoutChild {
        node: read_node(node, screen, depth + 1)?,
        size: read_size(child)?,
        min: extent(child, "min")?,
        max: optional_extent(child, "max")?,
        collapsible: boolean(child, "collapsible")?,
        collapse_priority: optional_priority(child, "collapse-priority")?,
    })
}

fn read_size(child: &TypedMap) -> Result<Size, String> {
    let TypedValue::Map(size) = field(child, "size")? else {
        return Err("a split child's size must be a table".to_owned());
    };
    if let Some(cells) = optional_extent(size, "fixed")? {
        return nonzero(cells, "size.fixed").map(Size::Fixed);
    }
    if let Some(share) = optional_extent(size, "weight")? {
        return nonzero(share, "size.weight").map(Size::Weight);
    }
    Err("a split child's size must declare fixed or weight".to_owned())
}

/// Reject a zero extent rather than correcting it.
///
/// A fixed size of zero and a weight of zero are unrepresentable in a
/// descriptor, so a document that asks for one is asking for a layout that
/// cannot exist; correcting it silently would produce geometry that does not
/// match the file it came from.
fn nonzero(value: u16, field: &str) -> Result<std::num::NonZeroU16, String> {
    std::num::NonZeroU16::new(value).ok_or_else(|| format!("{field} must be greater than zero"))
}

fn panel(screen: &ScreenDescriptor, name: &str) -> Result<PanelId, String> {
    screen
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == name)
        .map(|panel| panel.id)
        .ok_or_else(|| {
            format!("the layout names panel {name:?}, which this screen does not declare")
        })
}

fn field<'values>(values: &'values TypedMap, name: &str) -> Result<&'values TypedValue, String> {
    let key =
        Id::parse(name).map_err(|error| format!("{name} is not a configuration key: {error}"))?;
    values
        .get(&key)
        .ok_or_else(|| format!("the layout omits {name}"))
}

fn string<'values>(values: &'values TypedMap, name: &str) -> Result<&'values str, String> {
    match field(values, name)? {
        TypedValue::String(value) => Ok(value),
        _ => Err(format!("{name} must be a string")),
    }
}

fn boolean(values: &TypedMap, name: &str) -> Result<bool, String> {
    match field(values, name)? {
        TypedValue::Bool(value) => Ok(*value),
        _ => Err(format!("{name} must be true or false")),
    }
}

fn extent(values: &TypedMap, name: &str) -> Result<u16, String> {
    optional_extent(values, name)?.ok_or_else(|| format!("the layout omits {name}"))
}

fn optional_extent(values: &TypedMap, name: &str) -> Result<Option<u16>, String> {
    let key =
        Id::parse(name).map_err(|error| format!("{name} is not a configuration key: {error}"))?;
    match values.get(&key) {
        None => Ok(None),
        Some(TypedValue::Integer(value)) => u16::try_from(*value)
            .map(Some)
            .map_err(|_| format!("{name} must be a cell count")),
        Some(_) => Err(format!("{name} must be a whole number")),
    }
}

fn optional_priority(values: &TypedMap, name: &str) -> Result<Option<i32>, String> {
    let key =
        Id::parse(name).map_err(|error| format!("{name} is not a configuration key: {error}"))?;
    match values.get(&key) {
        None => Ok(None),
        Some(TypedValue::Integer(value)) => i32::try_from(*value)
            .map(Some)
            .map_err(|_| format!("{name} must be a collapse order key")),
        Some(_) => Err(format!("{name} must be a whole number")),
    }
}
