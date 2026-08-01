//! Structural-invariant tests for compiled descriptors (issue #384, CW04-01).
//!
//! Each test mutates one field of an otherwise valid descriptor so a failure
//! names exactly one violated rule.

use std::num::NonZeroU16;

use super::descriptor::{
    Axis, LayoutChild, LayoutNode, PanelDescriptor, PortDescriptor, PortDirection, PortRef,
    ScreenDescriptor, Size,
};
use super::ids::{
    MAX_PORTS_PER_PANEL, PanelId, PanelTypeId, PortId, RouteId, ScreenId, ScreenIdentity,
    VersionedTypeId,
};
use super::validate::{DescriptorError, validate_descriptor};

fn panel_id(value: &'static str) -> PanelId {
    PanelId::parse(value).unwrap_or_else(|_| unreachable!("fixture panel id {value} is valid"))
}

/// Generated panel identities for the split-child and depth fixtures.
const GENERATED_PANELS: [&str; 10] = [
    "panel-0", "panel-1", "panel-2", "panel-3", "panel-4", "panel-5", "panel-6", "panel-7",
    "panel-8", "panel-9",
];

fn make_panel(id: &'static str, focusable: bool, required: bool) -> PanelDescriptor {
    PanelDescriptor {
        id: panel_id(id),
        panel_type: PanelTypeId::parse("list")
            .unwrap_or_else(|_| unreachable!("fixture panel type is valid")),
        config: crate::domain::TypedMap::new(),
        focusable,
        required,
        ports: Vec::new(),
    }
}

fn weight(value: u16) -> Size {
    Size::Weight(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

fn child(id: &'static str, collapsible: bool, collapse_priority: Option<i32>) -> LayoutChild {
    LayoutChild {
        node: LayoutNode::Leaf {
            panel: panel_id(id),
        },
        size: weight(1),
        min: 3,
        max: None,
        collapsible,
        collapse_priority,
    }
}

/// A two-panel screen: `list` required+focusable, `detail` collapsible.
fn valid_descriptor() -> ScreenDescriptor {
    ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Dashboard),
        title: "Fixture".to_owned(),
        route: RouteId::parse("fixture")
            .unwrap_or_else(|_| unreachable!("fixture route id is valid")),
        panels: vec![
            make_panel("list", true, true),
            make_panel("detail", true, false),
        ],
        initial_focus: panel_id("list"),
        focus_order: vec![panel_id("list"), panel_id("detail")],
        relationships: Vec::new(),
        layout: LayoutNode::Split {
            axis: Axis::Vertical,
            gap: 1,
            children: vec![child("list", false, None), child("detail", true, Some(0))],
        },
    }
}

#[test]
fn the_fixture_descriptor_is_valid() {
    assert_eq!(validate_descriptor(&valid_descriptor()), Ok(()));
}

#[test]
fn a_screen_with_no_panels_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels.clear();
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::NoPanels { .. })
    ));
}

#[test]
fn a_duplicate_panel_identity_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels.push(make_panel("list", true, true));
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::DuplicatePanel { .. })
    ));
}

#[test]
fn a_declared_panel_missing_from_the_layout_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels.push(make_panel("orphan", false, false));
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::PanelNotInLayout { .. })
    ));
}

#[test]
fn a_layout_panel_that_is_not_declared_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.layout = LayoutNode::Split {
        axis: Axis::Vertical,
        gap: 1,
        children: vec![child("list", false, None), child("ghost", true, Some(0))],
    };
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::LayoutPanelNotDeclared { .. })
    ));
}

#[test]
fn placing_one_panel_twice_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.layout = LayoutNode::Split {
        axis: Axis::Vertical,
        gap: 1,
        children: vec![child("list", false, None), child("list", true, Some(0))],
    };
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::PanelPlacedTwice { .. })
    ));
}

#[test]
fn a_focusable_panel_missing_from_the_focus_order_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.focus_order = vec![panel_id("list")];
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::FocusOrderMissingPanel { .. })
    ));
}

#[test]
fn a_repeated_focus_order_entry_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.focus_order = vec![panel_id("list"), panel_id("detail"), panel_id("list")];
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::FocusOrderDuplicate { .. })
    ));
}

#[test]
fn a_non_focusable_panel_in_the_focus_order_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels = vec![
        make_panel("list", true, true),
        make_panel("detail", false, false),
    ];
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::FocusOrderUnfocusablePanel { .. })
    ));
}

#[test]
fn an_initial_focus_that_is_not_focusable_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels = vec![
        make_panel("list", true, true),
        make_panel("detail", false, false),
    ];
    descriptor.focus_order = vec![panel_id("list")];
    descriptor.initial_focus = panel_id("detail");
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::InitialFocusNotFocusable { .. })
    ));
}

#[test]
fn a_screen_with_no_required_focusable_panel_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels = vec![
        make_panel("list", true, false),
        make_panel("detail", true, false),
    ];
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::NoRequiredFocusablePanel { .. })
    ));
}

#[test]
fn a_split_with_fewer_than_two_children_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels = vec![make_panel("list", true, true)];
    descriptor.focus_order = vec![panel_id("list")];
    descriptor.layout = LayoutNode::Split {
        axis: Axis::Vertical,
        gap: 1,
        children: vec![child("list", false, None)],
    };
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::SplitChildCount { count: 1, .. })
    ));
}

#[test]
fn a_split_with_more_than_eight_children_is_rejected() {
    let mut descriptor = valid_descriptor();
    let ids = &GENERATED_PANELS[..9];
    descriptor.panels = ids
        .iter()
        .enumerate()
        .map(|(index, id)| make_panel(id, true, index == 0))
        .collect();
    descriptor.focus_order = ids.iter().copied().map(panel_id).collect();
    descriptor.initial_focus = panel_id(ids[0]);
    descriptor.layout = LayoutNode::Split {
        axis: Axis::Vertical,
        gap: 1,
        children: ids
            .iter()
            .enumerate()
            .map(|(index, id)| child(id, index > 0, (index > 0).then_some(0)))
            .collect(),
    };
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::SplitChildCount { count: 9, .. })
    ));
}

/// Build a right-nested chain of splits `depth` levels deep, placing one leaf
/// per level plus one at the bottom.
fn nested_descriptor(depth: usize) -> ScreenDescriptor {
    let ids = &GENERATED_PANELS[..=depth];
    let mut node = LayoutNode::Leaf {
        panel: panel_id(ids[depth]),
    };
    for id in ids[..depth].iter().rev() {
        node = LayoutNode::Split {
            axis: Axis::Vertical,
            gap: 1,
            children: vec![
                child(id, false, None),
                LayoutChild {
                    node,
                    size: weight(1),
                    min: 1,
                    max: None,
                    collapsible: false,
                    collapse_priority: None,
                },
            ],
        };
    }
    ScreenDescriptor {
        id: ScreenIdentity::Compiled(ScreenId::Dashboard),
        title: "Nested".to_owned(),
        route: RouteId::parse("nested")
            .unwrap_or_else(|_| unreachable!("fixture route id is valid")),
        panels: ids
            .iter()
            .enumerate()
            .map(|(index, id)| make_panel(id, true, index == 0))
            .collect(),
        initial_focus: panel_id(ids[0]),
        focus_order: ids.iter().copied().map(panel_id).collect(),
        relationships: Vec::new(),
        layout: node,
    }
}

#[test]
fn a_layout_nested_to_the_depth_limit_is_accepted() {
    // Eight levels of nesting: the outermost split is depth 1.
    assert_eq!(validate_descriptor(&nested_descriptor(7)), Ok(()));
}

#[test]
fn a_layout_nested_past_the_depth_limit_is_rejected() {
    assert!(matches!(
        validate_descriptor(&nested_descriptor(8)),
        Err(DescriptorError::LayoutTooDeep { .. })
    ));
}

#[test]
fn a_child_whose_max_is_below_its_min_is_rejected() {
    let mut descriptor = valid_descriptor();
    if let LayoutNode::Split { children, .. } = &mut descriptor.layout
        && let Some(first) = children.first_mut()
    {
        first.min = 10;
        first.max = Some(4);
    }
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::ChildMaxBelowMin {
            min: 10,
            max: 4,
            ..
        })
    ));
}

#[test]
fn a_collapsible_child_without_a_collapse_priority_is_rejected() {
    let mut descriptor = valid_descriptor();
    if let LayoutNode::Split { children, .. } = &mut descriptor.layout
        && let Some(last) = children.last_mut()
    {
        last.collapse_priority = None;
    }
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::CollapsiblePriorityMissing { .. })
    ));
}

#[test]
fn a_required_panel_under_a_collapsible_child_is_rejected() {
    let mut descriptor = valid_descriptor();
    descriptor.panels = vec![
        make_panel("list", true, true),
        make_panel("detail", true, true),
    ];
    assert!(matches!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::RequiredPanelCollapsible { .. })
    ));
}

#[test]
fn depth_first_panel_order_follows_declaration_order() {
    let descriptor = valid_descriptor();
    let order: Vec<&str> = descriptor
        .layout
        .panels_depth_first()
        .iter()
        .map(|id| id.as_str())
        .collect();
    assert_eq!(order, vec!["list", "detail"]);
}

#[test]
fn the_first_required_focusable_panel_follows_focus_order() {
    let descriptor = valid_descriptor();
    assert_eq!(
        descriptor
            .first_required_focusable()
            .map(|panel| panel.id.as_str()),
        Some("list")
    );
}

// ── Ports (issue #385) ─────────────────────────────────────────────────────

fn port(id: &'static str, direction: PortDirection, type_id: &'static str) -> PortDescriptor {
    PortDescriptor {
        id: PortId::parse(id)
            .unwrap_or_else(|error| unreachable!("fixture port id must parse: {error}")),
        direction,
        type_id: VersionedTypeId::parse(type_id)
            .unwrap_or_else(|error| unreachable!("fixture port type must parse: {error}")),
        required: false,
        retained: false,
    }
}

fn descriptor_with_ports(ports: Vec<PortDescriptor>) -> ScreenDescriptor {
    let mut descriptor = valid_descriptor();
    descriptor.panels[0].ports = ports;
    descriptor
}

#[test]
fn a_port_reference_resolves_only_against_the_panel_that_declares_it() {
    let descriptor = descriptor_with_ports(vec![port(
        "selection",
        PortDirection::Output,
        "github.issue@1",
    )]);

    let declared = PortRef {
        panel: panel_id("list"),
        port: PortId::parse("selection")
            .unwrap_or_else(|error| unreachable!("fixture port id must parse: {error}")),
    };
    let other_panel = PortRef {
        panel: panel_id("detail"),
        port: PortId::parse("selection")
            .unwrap_or_else(|error| unreachable!("fixture port id must parse: {error}")),
    };
    let unknown_port = PortRef {
        panel: panel_id("list"),
        port: PortId::parse("absent")
            .unwrap_or_else(|error| unreachable!("fixture port id must parse: {error}")),
    };

    assert_eq!(
        descriptor.port(&declared).map(|found| found.id.as_str()),
        Some("selection")
    );
    assert_eq!(descriptor.port(&other_panel), None);
    assert_eq!(descriptor.port(&unknown_port), None);
}

#[test]
fn a_panel_may_not_declare_one_port_identity_twice() {
    let descriptor = descriptor_with_ports(vec![
        port("selection", PortDirection::Output, "github.issue@1"),
        port("selection", PortDirection::Input, "github.issue@1"),
    ]);

    assert_eq!(
        validate_descriptor(&descriptor),
        Err(DescriptorError::DuplicatePort {
            screen: "core.dashboard",
            panel: "list",
            port: "selection",
        })
    );
}

#[test]
fn a_panel_may_declare_ports_up_to_the_limit_but_not_one_past_it() {
    let at_limit: Vec<PortDescriptor> = (0..MAX_PORTS_PER_PANEL)
        .map(|index| {
            let id = crate::workbench::intern::intern(&format!("port-{index}"))
                .unwrap_or_else(|error| unreachable!("fixture port id must intern: {error}"));
            port(id, PortDirection::Output, "github.issue@1")
        })
        .collect();
    let mut over_limit = at_limit.clone();
    over_limit.push(port("port-extra", PortDirection::Output, "github.issue@1"));

    assert_eq!(
        validate_descriptor(&descriptor_with_ports(at_limit)),
        Ok(())
    );
    assert_eq!(
        validate_descriptor(&descriptor_with_ports(over_limit)),
        Err(DescriptorError::TooManyPorts {
            screen: "core.dashboard",
            panel: "list",
            count: MAX_PORTS_PER_PANEL + 1,
        })
    );
}

#[test]
fn a_port_direction_renders_the_text_the_external_syntax_uses() {
    assert_eq!(PortDirection::Input.as_str(), "input");
    assert_eq!(PortDirection::Output.as_str(), "output");
}
