//! Every declared parser bound at its limit and one past it (issue #385,
//! CW05-02).

use crate::persistence::diagnostic::{ARRAY_LIMIT, MAP_LIMIT, NESTING_LIMIT, STRING_LIMIT};

use super::ids::{
    ID_BYTE_LIMIT, MAX_ACTIVATION_FIELDS, MAX_BINDINGS_PER_SCREEN, MAX_LAYOUT_DEPTH,
    MAX_PANELS_PER_SCREEN, MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN, MAX_SPLIT_CHILDREN,
    MIN_SPLIT_CHILDREN,
};
use super::screen_file::parse_screen_file;
use super::screen_file_bounds::ScreenSyntaxReason;
use super::screen_file_fixtures::{
    HEADER, LAYOUT, PANELS, leaf_layout, rejected, single_panel_text, valid_text,
};

// ── Structural bounds, at the limit and one past it ────────────────────────

/// Build `count` panels named `p0..p<count-1>`.
fn panels_text(count: usize) -> String {
    (0..count)
        .map(|index| {
            format!(
                "\n[[panels]]\nid = \"p{index}\"\ntype = \"list\"\nfocusable = true\nrequired = {}\n",
                index == 0
            )
        })
        .collect::<Vec<String>>()
        .concat()
}

/// An inline leaf node naming panel `p<index>`.
fn inline_leaf(index: usize) -> String {
    format!("{{ type = \"leaf\", panel = \"p{index}\" }}")
}

/// An inline child wrapping `node`.
fn inline_child(node: &str) -> String {
    format!("{{ min = 1, collapsible = false, size = {{ weight = 1 }}, node = {node} }}")
}

/// An inline split over the given child nodes.
fn inline_split(nodes: &[String]) -> String {
    let children: Vec<String> = nodes.iter().map(|node| inline_child(node)).collect();
    format!(
        "{{ type = \"split\", axis = \"vertical\", children = [{}] }}",
        children.join(", ")
    )
}

/// A layout placing every panel exactly once, grouped so no split exceeds its
/// child limit.
fn balanced_layout(count: usize) -> String {
    let leaves: Vec<String> = (0..count).map(inline_leaf).collect();
    let node = if count == 1 {
        leaves[0].clone()
    } else if count <= MAX_SPLIT_CHILDREN {
        inline_split(&leaves)
    } else {
        let groups: Vec<String> = leaves
            .chunks(MAX_SPLIT_CHILDREN)
            .map(|group| {
                if group.len() == 1 {
                    group[0].clone()
                } else {
                    inline_split(group)
                }
            })
            .collect();
        inline_split(&groups)
    };
    format!("layout = {node}\n")
}

fn screen_of_panels(count: usize) -> String {
    format!(
        "screen_schema = 1\nid = \"local.review\"\ntitle = \"R\"\nroute = \"review\"\ninitial_focus = \"p0\"\nfocus_order = [\"p0\"]\n{}{}",
        balanced_layout(count),
        panels_text(count)
    )
}

/// A screen whose root split has exactly `count` direct leaf children.
fn screen_with_split_children(count: usize) -> String {
    let leaves: Vec<String> = (0..count).map(inline_leaf).collect();
    format!(
        "screen_schema = 1\nid = \"local.review\"\ntitle = \"R\"\nroute = \"review\"\ninitial_focus = \"p0\"\nfocus_order = [\"p0\"]\nlayout = {}\n{}",
        inline_split(&leaves),
        panels_text(count.max(1))
    )
}

#[test]
fn a_screen_may_declare_panels_up_to_the_limit_but_not_one_past_it() {
    let at_limit = screen_of_panels(MAX_PANELS_PER_SCREEN);
    assert!(
        parse_screen_file(&at_limit).is_ok(),
        "{:?}",
        parse_screen_file(&at_limit).err()
    );
    assert_eq!(
        rejected(&screen_of_panels(MAX_PANELS_PER_SCREEN + 1)),
        ScreenSyntaxReason::PanelCount {
            count: MAX_PANELS_PER_SCREEN + 1
        }
    );
}

#[test]
fn a_screen_must_declare_at_least_one_panel() {
    let text = format!(
        "screen_schema = 1\nid = \"local.review\"\ntitle = \"R\"\nroute = \"review\"\ninitial_focus = \"p0\"\nfocus_order = []\npanels = []\n{}",
        leaf_layout("p0")
    );

    assert_eq!(rejected(&text), ScreenSyntaxReason::PanelCount { count: 0 });
}

fn screen_with_ports(count: usize) -> String {
    let ports: String = (0..count)
        .map(|index| {
            format!(
                "\n[[panels.ports]]\nid = \"port{index}\"\ndirection = \"output\"\ntype_id = \"t@1\"\nrequired = false\nretained = false\n"
            )
        })
        .collect::<Vec<String>>()
        .concat();
    single_panel_text(&ports, &leaf_layout("pr-list"))
}

#[test]
fn a_panel_may_declare_ports_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_ports(MAX_PORTS_PER_PANEL)).is_ok());
    assert_eq!(
        rejected(&screen_with_ports(MAX_PORTS_PER_PANEL + 1)),
        ScreenSyntaxReason::PortCount {
            count: MAX_PORTS_PER_PANEL + 1
        }
    );
}

fn screen_with_relationships(count: usize) -> String {
    let relationships = "\n[[relationships]]\nkind = \"scope\"\nsource = \"pr-list.selection\"\ntarget = \"pr-detail.subject\"\n".repeat(count);
    format!("{HEADER}{PANELS}{LAYOUT}{relationships}")
}

#[test]
fn a_screen_may_declare_relationships_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_relationships(MAX_RELATIONSHIPS_PER_SCREEN)).is_ok());
    assert_eq!(
        rejected(&screen_with_relationships(MAX_RELATIONSHIPS_PER_SCREEN + 1)),
        ScreenSyntaxReason::RelationshipCount {
            count: MAX_RELATIONSHIPS_PER_SCREEN + 1
        }
    );
}

fn screen_with_activation(count: usize) -> String {
    let fields: String = (0..count)
        .map(|index| format!("\n[[activation]]\nname = \"field{index}\"\ntype = \"boolean\"\n"))
        .collect::<Vec<String>>()
        .concat();
    format!("{HEADER}{PANELS}{LAYOUT}{fields}")
}

#[test]
fn a_screen_may_declare_activation_fields_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_activation(MAX_ACTIVATION_FIELDS)).is_ok());
    assert_eq!(
        rejected(&screen_with_activation(MAX_ACTIVATION_FIELDS + 1)),
        ScreenSyntaxReason::ActivationFieldCount {
            count: MAX_ACTIVATION_FIELDS + 1
        }
    );
}

fn screen_with_bindings(count: usize) -> String {
    let bindings: String = (0..count)
        .map(|index| {
            format!("\n[[bindings]]\ncontext = \"pull-requests\"\naction = \"action{index}\"\n")
        })
        .collect::<Vec<String>>()
        .concat();
    format!("{HEADER}{PANELS}{LAYOUT}{bindings}")
}

#[test]
fn a_screen_may_declare_bindings_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_bindings(MAX_BINDINGS_PER_SCREEN)).is_ok());
    assert_eq!(
        rejected(&screen_with_bindings(MAX_BINDINGS_PER_SCREEN + 1)),
        ScreenSyntaxReason::BindingCount {
            count: MAX_BINDINGS_PER_SCREEN + 1
        }
    );
}

#[test]
fn a_split_declares_between_two_and_eight_children_inclusive() {
    assert!(parse_screen_file(&screen_with_split_children(MIN_SPLIT_CHILDREN)).is_ok());
    assert!(parse_screen_file(&screen_with_split_children(MAX_SPLIT_CHILDREN)).is_ok());
    assert_eq!(
        rejected(&screen_with_split_children(MIN_SPLIT_CHILDREN - 1)),
        ScreenSyntaxReason::SplitChildCount {
            count: MIN_SPLIT_CHILDREN - 1
        }
    );
    assert_eq!(
        rejected(&screen_with_split_children(MAX_SPLIT_CHILDREN + 1)),
        ScreenSyntaxReason::SplitChildCount {
            count: MAX_SPLIT_CHILDREN + 1
        }
    );
}

/// A layout nested `levels` deep, where level 1 is the root split.
fn nested_layout(levels: usize) -> String {
    let mut node = String::from("{ type = \"leaf\", panel = \"pr-detail\" }");
    for _ in 1..levels {
        node = format!(
            "{{ type = \"split\", axis = \"vertical\", children = [ {{ min = 1, collapsible = false, size = {{ weight = 1 }}, node = {{ type = \"leaf\", panel = \"pr-list\" }} }}, {{ min = 1, collapsible = false, size = {{ weight = 1 }}, node = {node} }} ] }}"
        );
    }
    format!("layout = {node}\n")
}

#[test]
fn a_layout_may_nest_to_the_depth_limit_but_not_one_past_it() {
    let at_limit = format!("{HEADER}{}{PANELS}", nested_layout(MAX_LAYOUT_DEPTH));
    let over_limit = format!("{HEADER}{}{PANELS}", nested_layout(MAX_LAYOUT_DEPTH + 1));

    assert!(
        parse_screen_file(&at_limit).is_ok(),
        "{:?}",
        parse_screen_file(&at_limit).err()
    );
    assert_eq!(
        rejected(&over_limit),
        ScreenSyntaxReason::LayoutTooDeep {
            depth: MAX_LAYOUT_DEPTH + 1
        }
    );
}

#[test]
fn a_zero_size_minimum_or_maximum_is_rejected() {
    for (from, to, field) in [
        (
            "size = { weight = 1 }",
            "size = { weight = 0 }",
            "size.weight",
        ),
        (
            "size = { weight = 1 }",
            "size = { fixed = 0 }",
            "size.fixed",
        ),
        ("min = 20", "min = 0", "min"),
    ] {
        let text = valid_text().replacen(from, to, 1);
        assert_eq!(
            rejected(&text),
            ScreenSyntaxReason::ZeroExtent { field },
            "{to} must be rejected"
        );
    }
}

#[test]
fn a_maximum_below_the_minimum_is_rejected() {
    let text = valid_text().replacen("min = 20", "min = 20\nmax = 19", 1);

    assert_eq!(
        rejected(&text),
        ScreenSyntaxReason::MaxBelowMin { min: 20, max: 19 }
    );
}

#[test]
fn a_maximum_equal_to_the_minimum_is_accepted() {
    let text = valid_text().replacen("min = 20", "min = 20\nmax = 20", 1);

    assert!(parse_screen_file(&text).is_ok());
}

#[test]
fn a_collapse_priority_must_be_present_exactly_when_collapsible() {
    let missing = valid_text().replacen(
        "collapsible = true\ncollapse_priority = 0",
        "collapsible = true",
        1,
    );
    let spurious = valid_text().replacen(
        "collapsible = false",
        "collapsible = false\ncollapse_priority = 3",
        1,
    );

    assert_eq!(
        rejected(&missing),
        ScreenSyntaxReason::CollapsePriorityMismatch { collapsible: true }
    );
    assert_eq!(
        rejected(&spurious),
        ScreenSyntaxReason::CollapsePriorityMismatch { collapsible: false }
    );
}

#[test]
fn enum_values_must_be_present_exactly_for_enum_fields() {
    let missing =
        format!("{HEADER}{PANELS}{LAYOUT}\n[[activation]]\nname = \"mode\"\ntype = \"enum\"\n");
    let spurious = format!(
        "{HEADER}{PANELS}{LAYOUT}\n[[activation]]\nname = \"flag\"\ntype = \"boolean\"\nvalues = [\"a\"]\n"
    );

    assert_eq!(
        rejected(&missing),
        ScreenSyntaxReason::EnumValuesMismatch { is_enum: true }
    );
    assert_eq!(
        rejected(&spurious),
        ScreenSyntaxReason::EnumValuesMismatch { is_enum: false }
    );
}

// ── Generic value bounds, at the limit and one past it ─────────────────────

#[test]
fn an_identifier_may_reach_the_byte_limit_but_not_pass_it() {
    let at_limit = "a".repeat(ID_BYTE_LIMIT);
    let over_limit = "a".repeat(ID_BYTE_LIMIT + 1);

    assert!(
        parse_screen_file(&valid_text().replacen(
            "route = \"review\"",
            &format!("route = \"{at_limit}\""),
            1
        ))
        .is_ok()
    );
    assert_eq!(
        rejected(&valid_text().replacen(
            "route = \"review\"",
            &format!("route = \"{over_limit}\""),
            1
        )),
        ScreenSyntaxReason::IdentifierTooLong {
            field: "route",
            bytes: ID_BYTE_LIMIT + 1
        }
    );
}

#[test]
fn a_string_may_reach_the_byte_limit_but_not_pass_it() {
    let at_limit = "t".repeat(STRING_LIMIT);
    let over_limit = "t".repeat(STRING_LIMIT + 1);

    assert!(
        parse_screen_file(&valid_text().replacen(
            "title = \"Review\"",
            &format!("title = \"{at_limit}\""),
            1
        ))
        .is_ok()
    );
    assert_eq!(
        rejected(&valid_text().replacen(
            "title = \"Review\"",
            &format!("title = \"{over_limit}\""),
            1
        )),
        ScreenSyntaxReason::StringTooLong {
            bytes: STRING_LIMIT + 1
        }
    );
}

fn screen_with_config_entries(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("e{index} = {index}"))
        .collect();
    let config = format!("\n[panels.config]\n{}\n", entries.join("\n"));
    single_panel_text(&config, &leaf_layout("pr-list"))
}

#[test]
fn a_map_may_hold_entries_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_config_entries(MAP_LIMIT)).is_ok());
    assert_eq!(
        rejected(&screen_with_config_entries(MAP_LIMIT + 1)),
        ScreenSyntaxReason::MapTooLarge {
            entries: MAP_LIMIT + 1
        }
    );
}

fn screen_with_config_array(count: usize) -> String {
    let elements: Vec<String> = (0..count).map(|index| index.to_string()).collect();
    let config = format!("\n[panels.config]\nvalues = [{}]\n", elements.join(","));
    single_panel_text(&config, &leaf_layout("pr-list"))
}

#[test]
fn an_array_may_hold_elements_up_to_the_limit_but_not_one_past_it() {
    assert!(parse_screen_file(&screen_with_config_array(ARRAY_LIMIT)).is_ok());
    assert_eq!(
        rejected(&screen_with_config_array(ARRAY_LIMIT + 1)),
        ScreenSyntaxReason::ArrayTooLarge {
            elements: ARRAY_LIMIT + 1
        }
    );
}

/// Config data nested `levels` deep below the panel config table.
fn screen_with_config_depth(levels: usize) -> String {
    let mut value = String::from("1");
    for _ in 0..levels {
        value = format!("{{ nested = {value} }}");
    }
    let config = format!("\n[panels.config]\nroot = {value}\n");
    single_panel_text(&config, &leaf_layout("pr-list"))
}

#[test]
fn data_may_nest_to_the_document_limit_but_not_one_past_it() {
    // The panel config table sits at depth 3 (document, panel, config), so the
    // deepest legal value nests NESTING_LIMIT - 3 further levels.
    let head_room = NESTING_LIMIT - 3;

    assert!(
        parse_screen_file(&screen_with_config_depth(head_room)).is_ok(),
        "{:?}",
        parse_screen_file(&screen_with_config_depth(head_room)).err()
    );
    assert_eq!(
        rejected(&screen_with_config_depth(head_room + 1)),
        ScreenSyntaxReason::DocumentTooDeep {
            depth: NESTING_LIMIT + 1
        }
    );
}
