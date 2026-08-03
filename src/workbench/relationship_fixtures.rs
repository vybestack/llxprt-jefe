//! Descriptor fixtures shared by the relationship graph and propagation tests
//! (issue #385).

use std::num::NonZeroU16;

use crate::domain::TypedMap;

use super::descriptor::{
    Axis, LayoutChild, LayoutNode, PanelDescriptor, PortDescriptor, PortDirection, PortRef,
    ScreenDescriptor, Size,
};
use super::ids::{
    CustomScreenId, PanelId, PanelTypeId, PortId, RouteId, ScreenIdentity, VersionedTypeId,
};
use super::intern::intern;
use super::relationships::Relationship;

/// The versioned type every fixture port carries unless told otherwise.
pub const SUBJECT_TYPE: &str = "github.pull-request@1";

pub fn panel_id(value: &str) -> PanelId {
    PanelId::parse(interned(value))
        .unwrap_or_else(|error| unreachable!("fixture panel id must parse: {error}"))
}

pub fn port_id(value: &str) -> PortId {
    PortId::parse(interned(value))
        .unwrap_or_else(|error| unreachable!("fixture port id must parse: {error}"))
}

pub fn type_id(value: &str) -> VersionedTypeId {
    VersionedTypeId::parse(interned(value))
        .unwrap_or_else(|error| unreachable!("fixture type id must parse: {error}"))
}

pub fn port_ref(panel: &str, port: &str) -> PortRef {
    PortRef {
        panel: panel_id(panel),
        port: port_id(port),
    }
}

fn interned(value: &str) -> &'static str {
    intern(value).unwrap_or_else(|error| unreachable!("fixture identifier must intern: {error}"))
}

/// A port declaration with every knob explicit.
pub fn port(id: &str, direction: PortDirection, type_text: &str, retained: bool) -> PortDescriptor {
    PortDescriptor {
        id: port_id(id),
        direction,
        type_id: type_id(type_text),
        required: false,
        retained,
    }
}

/// A focusable panel with the given ports.
pub fn panel(id: &str, required: bool, ports: Vec<PortDescriptor>) -> PanelDescriptor {
    PanelDescriptor {
        id: panel_id(id),
        panel_type: PanelTypeId::parse("list")
            .unwrap_or_else(|error| unreachable!("fixture panel type must parse: {error}")),
        config: TypedMap::new(),
        focusable: true,
        required,
        ports,
    }
}

/// A screen laying every panel out side by side, with the given relationships.
///
/// The first panel is required and focused, so the descriptor satisfies the
/// structural invariants without the relationship tests having to restate them.
pub fn screen(panels: Vec<PanelDescriptor>, relationships: Vec<Relationship>) -> ScreenDescriptor {
    let ids: Vec<PanelId> = panels.iter().map(|panel| panel.id).collect();
    let layout = if ids.len() == 1 {
        LayoutNode::Leaf { panel: ids[0] }
    } else {
        LayoutNode::Split {
            axis: Axis::Horizontal,
            gap: 0,
            children: ids
                .iter()
                .map(|id| LayoutChild {
                    node: LayoutNode::Leaf { panel: *id },
                    size: Size::Weight(NonZeroU16::MIN),
                    min: 1,
                    max: None,
                    collapsible: false,
                    collapse_priority: None,
                })
                .collect(),
        }
    };
    ScreenDescriptor {
        id: ScreenIdentity::Custom(
            CustomScreenId::parse("local.review")
                .unwrap_or_else(|error| unreachable!("fixture screen id must parse: {error}")),
        ),
        title: "Review".to_owned(),
        route: RouteId::parse("review")
            .unwrap_or_else(|error| unreachable!("fixture route must parse: {error}")),
        initial_focus: ids[0],
        focus_order: ids,
        panels,
        relationships,
        activation: Vec::new(),
        bindings: Vec::new(),
        layout,
    }
}

/// The canonical two-panel list/detail screen: one output, one input.
///
/// `retained` controls whether the detail input keeps its value when its source
/// goes absent, which is the switch the empty-policy tests turn.
pub fn list_detail(
    relationship_kind: super::relationships::RelationshipKind,
    retained: bool,
) -> ScreenDescriptor {
    screen(
        vec![
            panel(
                "list",
                true,
                vec![port(
                    "selection",
                    PortDirection::Output,
                    SUBJECT_TYPE,
                    false,
                )],
            ),
            panel(
                "detail",
                false,
                vec![port(
                    "subject",
                    PortDirection::Input,
                    SUBJECT_TYPE,
                    retained,
                )],
            ),
        ],
        vec![Relationship {
            kind: relationship_kind,
            source: port_ref("list", "selection"),
            target: port_ref("detail", "subject"),
        }],
    )
}
