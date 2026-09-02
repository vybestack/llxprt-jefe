//! Behavioral tests for the settings a composed screen registry honours
//! (issue #388).
//!
//! The Screens/Layout editor writes `workbench.layout_overrides` and
//! `workbench.screen_order`. These are the tests that say what a restart then
//! does with them.

use std::collections::BTreeMap;

use crate::domain::{Id, TypedMap, TypedValue};
use crate::persistence::settings_document::{PublishedSettings, PublishedWorkbenchSettings};
use crate::workbench::descriptor::LayoutNode;
use crate::workbench::screens::builtin_screens;

use super::compose::compose_screens;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("id fixture {value}: {error}"))
}

fn key(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("key fixture {value}: {error}"))
}

/// A layout override naming one panel of the screen it overrides.
fn leaf_override(panel: &str) -> TypedMap {
    let mut map = TypedMap::new();
    map.insert(key("type"), TypedValue::String("leaf".to_owned()));
    map.insert(key("panel"), TypedValue::String(panel.to_owned()));
    map
}

/// One split child claiming a share of its parent.
fn weighted_child(panel: &str) -> TypedValue {
    let mut size = TypedMap::new();
    size.insert(key("weight"), TypedValue::Integer(1));
    let mut child = TypedMap::new();
    child.insert(key("node"), TypedValue::Map(leaf_override(panel)));
    child.insert(key("size"), TypedValue::Map(size));
    child.insert(key("min"), TypedValue::Integer(0));
    child.insert(key("collapsible"), TypedValue::Bool(false));
    TypedValue::Map(child)
}

/// A layout override splitting the named panels along one axis.
///
/// Every declared panel appears exactly once, which is what makes an override
/// something the descriptor validator will accept.
fn split_override(axis: &str, panels: &[&str]) -> TypedMap {
    let mut map = TypedMap::new();
    map.insert(key("type"), TypedValue::String("split".to_owned()));
    map.insert(key("axis"), TypedValue::String(axis.to_owned()));
    map.insert(
        key("children"),
        TypedValue::List(panels.iter().map(|panel| weighted_child(panel)).collect()),
    );
    map
}

/// The Repositories screen, which declares exactly four panels.
fn repositories_screen_fixture(
    compiled: &crate::workbench::screens::ScreenRegistry,
) -> &crate::workbench::descriptor::ScreenDescriptor {
    compiled
        .screens()
        .iter()
        .find(|screen| screen.id.as_str() == "core.repositories")
        .unwrap_or_else(|| panic!("the Repositories screen"))
}

fn settings(workbench: PublishedWorkbenchSettings) -> PublishedSettings {
    PublishedSettings {
        workbench,
        ..PublishedSettings::default()
    }
}

#[test]
fn a_saved_layout_override_is_the_layout_the_registry_publishes() {
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let screen = repositories_screen_fixture(&compiled);
    let mut overrides = BTreeMap::new();
    overrides.insert(
        id(screen.id.as_str()),
        split_override("horizontal", &["repositories", "status", "cards", "filter"]),
    );

    let composed = compose_screens(
        &compiled,
        &[],
        &settings(PublishedWorkbenchSettings {
            layout_overrides: overrides,
            ..PublishedWorkbenchSettings::default()
        }),
    )
    .unwrap_or_else(|refusal| panic!("a valid override composes: {refusal}"));

    let published = composed
        .registry
        .get_identity(screen.id)
        .unwrap_or_else(|| panic!("the overridden screen"));
    let LayoutNode::Split { axis, children, .. } = &published.layout else {
        panic!("the saved override is a split");
    };
    assert_eq!(
        *axis,
        crate::workbench::descriptor::Axis::Horizontal,
        "a restart draws the layout the user saved, not the compiled one"
    );
    assert_eq!(
        children
            .iter()
            .map(|child| match &child.node {
                LayoutNode::Leaf { panel } => panel.as_str(),
                LayoutNode::Split { .. } => panic!("the fixture places leaves"),
            })
            .collect::<Vec<_>>(),
        vec!["repositories", "status", "cards", "filter"],
        "in the order the user saved"
    );
}

#[test]
fn a_screen_with_no_override_keeps_the_layout_it_was_compiled_with() {
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let screen = compiled
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let expected = screen.layout.clone();

    let composed = compose_screens(&compiled, &[], &PublishedSettings::default())
        .unwrap_or_else(|refusal| panic!("no override composes: {refusal}"));

    assert_eq!(
        composed
            .registry
            .get_identity(screen.id)
            .map(|descriptor| descriptor.layout.clone()),
        Some(expected)
    );
}

#[test]
fn an_override_the_validator_refuses_warns_and_leaves_the_compiled_layout() {
    // Startup must not be held hostage by a layout the user can only correct
    // from inside the program. The compiled layout stands, the reason is
    // reported, and the Settings editor shows the row as invalid.
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let screen = repositories_screen_fixture(&compiled);
    let expected = screen.layout.clone();
    let mut overrides = BTreeMap::new();
    overrides.insert(id(screen.id.as_str()), leaf_override("nothing-declares-me"));

    let composed = compose_screens(
        &compiled,
        &[],
        &settings(PublishedWorkbenchSettings {
            layout_overrides: overrides,
            ..PublishedWorkbenchSettings::default()
        }),
    )
    .unwrap_or_else(|refusal| panic!("a bad override must not stop startup: {refusal}"));

    assert_eq!(
        composed
            .registry
            .get_identity(screen.id)
            .map(|descriptor| descriptor.layout.clone()),
        Some(expected),
        "the compiled layout stands"
    );
    assert!(
        composed
            .warnings
            .iter()
            .any(|warning| warning.path.as_str().contains(screen.id.as_str())),
        "and the reason names the screen: {:?}",
        composed.warnings
    );
}

#[test]
fn an_override_naming_no_known_screen_is_reported_rather_than_ignored() {
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let mut overrides = BTreeMap::new();
    overrides.insert(id("local.nothing-declares-me"), leaf_override("list"));

    let composed = compose_screens(
        &compiled,
        &[],
        &settings(PublishedWorkbenchSettings {
            layout_overrides: overrides,
            ..PublishedWorkbenchSettings::default()
        }),
    )
    .unwrap_or_else(|refusal| panic!("an orphan override must not stop startup: {refusal}"));

    assert!(
        composed
            .warnings
            .iter()
            .any(|warning| warning.path.as_str().contains("local.nothing-declares-me")),
        "an override for a screen that is not there is said out loud: {:?}",
        composed.warnings
    );
}

#[test]
fn a_saved_screen_order_is_the_order_the_registry_publishes() {
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let last = compiled
        .screens()
        .last()
        .unwrap_or_else(|| panic!("a compiled screen"))
        .id;

    let composed = compose_screens(
        &compiled,
        &[],
        &settings(PublishedWorkbenchSettings {
            screen_order: vec![id(last.as_str())],
            ..PublishedWorkbenchSettings::default()
        }),
    )
    .unwrap_or_else(|refusal| panic!("an order composes: {refusal}"));

    assert_eq!(
        composed.registry.screens().first().map(|screen| screen.id),
        Some(last),
        "the screen the user ordered first leads"
    );
    assert_eq!(
        composed.registry.screens().len(),
        compiled.screens().len(),
        "ordering keeps every screen"
    );
}

#[test]
fn an_order_naming_only_some_screens_leaves_the_rest_in_compiled_order() {
    let compiled = builtin_screens().unwrap_or_else(|error| panic!("compiled table: {error}"));
    let expected: Vec<_> = compiled
        .screens()
        .iter()
        .map(|screen| screen.id)
        .filter(|screen| *screen != compiled.screens()[2].id)
        .collect();
    let moved = compiled.screens()[2].id;

    let composed = compose_screens(
        &compiled,
        &[],
        &settings(PublishedWorkbenchSettings {
            screen_order: vec![id(moved.as_str())],
            ..PublishedWorkbenchSettings::default()
        }),
    )
    .unwrap_or_else(|refusal| panic!("a partial order composes: {refusal}"));

    let published: Vec<_> = composed
        .registry
        .screens()
        .iter()
        .map(|screen| screen.id)
        .collect();
    assert_eq!(published.first(), Some(&moved));
    assert_eq!(
        &published[1..],
        expected.as_slice(),
        "the screens the order does not name keep the order they had"
    );
}
