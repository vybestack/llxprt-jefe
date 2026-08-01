//! Shipped-screen parity tests (issue #384, CW04-01).
//!
//! The compiled table is compared against a checked-in golden so a change to
//! any screen's identity, panel set, focus order, requiredness, or collapse
//! order has to be made deliberately in both places.

use serde_json::Value;

use super::descriptor::{LayoutChild, LayoutNode, ScreenDescriptor};
use super::ids::{MAX_PANELS_PER_SCREEN, PanelId, ScreenId};
use super::screens::{PTY_PANEL_TYPE, ScreenRegistry, builtin_screens};
use super::validate::validate_descriptor;

const PARITY_GOLDEN: &str = include_str!("shipped-screen-definition-parity.json");

fn registry() -> ScreenRegistry {
    builtin_screens().unwrap_or_else(|error| unreachable!("compiled screens are valid: {error}"))
}

/// Collapse candidates in the order the resolver would hide them:
/// `(collapse_priority ascending, depth_first_index descending)`.
fn collapse_order(descriptor: &ScreenDescriptor) -> Vec<String> {
    let mut candidates: Vec<(i32, usize, String)> = Vec::new();
    collect_collapsible(&descriptor.layout, &mut 0, &mut candidates);
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then(right.1.cmp(&left.1)));
    candidates.into_iter().map(|(_, _, id)| id).collect()
}

fn collect_collapsible(
    node: &LayoutNode,
    next_index: &mut usize,
    collected: &mut Vec<(i32, usize, String)>,
) {
    match node {
        LayoutNode::Leaf { .. } => *next_index += 1,
        LayoutNode::Split { children, .. } => {
            for child in children {
                let index = *next_index;
                if child.collapsible {
                    collected.push((
                        child.collapse_priority.unwrap_or(0),
                        index,
                        child
                            .node
                            .panels_depth_first()
                            .first()
                            .map_or_else(String::new, |panel| panel.as_str().to_owned()),
                    ));
                }
                collect_collapsible(&child.node, next_index, collected);
            }
        }
    }
}

fn golden() -> Value {
    serde_json::from_str(PARITY_GOLDEN)
        .unwrap_or_else(|error| unreachable!("parity golden is valid JSON: {error}"))
}

#[test]
fn the_registry_contains_exactly_the_five_parity_screens() {
    let registry = registry();
    let ids: Vec<&str> = registry
        .screens()
        .iter()
        .map(|screen| screen.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "core.dashboard",
            "core.repositories",
            "github.issues",
            "github.pull-requests",
            "github.actions",
        ]
    );
}

#[test]
fn every_compiled_descriptor_validates() {
    for screen in registry().screens() {
        assert_eq!(
            validate_descriptor(screen),
            Ok(()),
            "screen {} must validate",
            screen.id
        );
    }
}

#[test]
fn screen_identities_are_unique() {
    let registry = registry();
    for (index, screen) in registry.screens().iter().enumerate() {
        assert!(
            !registry.screens()[..index]
                .iter()
                .any(|prior| prior.id == screen.id),
            "screen {} is declared twice",
            screen.id
        );
    }
}

#[test]
fn panel_identities_are_unique_within_each_screen() {
    for screen in registry().screens() {
        for (index, panel) in screen.panels.iter().enumerate() {
            assert!(
                !screen.panels[..index]
                    .iter()
                    .any(|prior| prior.id == panel.id),
                "screen {} declares panel {} twice",
                screen.id,
                panel.id
            );
        }
        assert!(screen.panels.len() <= MAX_PANELS_PER_SCREEN);
    }
}

#[test]
fn the_first_screen_is_the_compiled_initial_screen() {
    let registry = registry();
    assert_eq!(
        registry.initial_screen().map(|screen| screen.id.as_str()),
        Some("core.dashboard")
    );
}

#[test]
fn lookup_by_stable_identity_finds_each_screen() {
    let registry = registry();
    for screen in registry.screens() {
        let looked_up = ScreenId::parse(screen.id.as_str())
            .ok()
            .and_then(|id| registry.get(&id));
        assert_eq!(looked_up.map(|found| &found.id), Some(&screen.id));
    }
}

#[test]
fn only_the_repositories_screen_declares_a_pty_panel() {
    for screen in registry().screens() {
        let pty_panels: Vec<&str> = screen
            .panels
            .iter()
            .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
            .map(|panel| panel.id.as_str())
            .collect();
        let expected: Vec<&str> = if screen.id.as_str() == "core.repositories" {
            vec!["terminal"]
        } else {
            Vec::new()
        };
        assert_eq!(pty_panels, expected, "screen {}", screen.id);
    }
}

#[test]
fn compiled_screens_match_the_parity_golden() {
    let expected = golden();
    let Some(expected_screens) = expected.get("screens").and_then(Value::as_array) else {
        unreachable!("parity golden has a screens array");
    };
    let registry = registry();
    assert_eq!(
        registry.screens().len(),
        expected_screens.len(),
        "screen count must match the golden"
    );

    for (screen, expected) in registry.screens().iter().zip(expected_screens) {
        assert_eq!(
            Some(screen.id.as_str()),
            expected.get("id").and_then(Value::as_str)
        );
        assert_eq!(
            Some(screen.title.as_str()),
            expected.get("title").and_then(Value::as_str)
        );
        assert_eq!(
            Some(screen.route.as_str()),
            expected.get("route").and_then(Value::as_str)
        );
        assert_eq!(
            Some(screen.initial_focus.as_str()),
            expected.get("initial_focus").and_then(Value::as_str)
        );

        let focus: Vec<&str> = screen.focus_order.iter().map(PanelId::as_str).collect();
        assert_eq!(Some(focus), string_list(expected, "focus_order"));

        let required: Vec<&str> = screen
            .panels
            .iter()
            .filter(|panel| panel.required)
            .map(|panel| panel.id.as_str())
            .collect();
        assert_eq!(Some(required), string_list(expected, "required_panels"));

        assert_eq!(
            Some(collapse_order(screen)),
            string_list(expected, "collapse_order")
                .map(|values| values.into_iter().map(str::to_owned).collect())
        );
    }
}

fn string_list<'value>(value: &'value Value, key: &str) -> Option<Vec<&'value str>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(Value::as_str).collect())
}

#[test]
fn every_layout_child_declares_a_usable_minimum() {
    for screen in registry().screens() {
        assert_children(&screen.layout, &mut |child| {
            assert!(
                child.min >= 1,
                "screen {} declares a child with a zero minimum",
                screen.id
            );
            if let Some(max) = child.max {
                assert!(
                    max >= child.min,
                    "screen {} declares max below min",
                    screen.id
                );
            }
        });
    }
}

fn assert_children(node: &LayoutNode, check: &mut impl FnMut(&LayoutChild)) {
    if let LayoutNode::Split { children, .. } = node {
        for child in children {
            check(child);
            assert_children(&child.node, check);
        }
    }
}
