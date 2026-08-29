//! Open-screen navigation: a route targeting a composed Package screen is
//! accepted by the same Push/Replace/Back reducer as compiled screens
//! (issue #391, CW-11 Slice A — navigation sub-slice).
//!
//! These tests prove the navigation seam is open rather than rejecting every
//! non-compiled target. A Package screen lowered from a selected package's
//! descriptor is already part of the same `ScreenRegistry` as the compiled
//! screens; navigation must reach it atomically (Push suspends, Replace
//! disposes, Back restores) and focus it where its *descriptor* says, not where
//! the compiled focus table assumes.
//!
//! Identity assertions use the stable identity string (`as_str`) rather than a
//! typed `ScreenIdentity` comparison, so the behavioral gate reads identically
//! before and after the `ScreenInstance.screen` field is widened.

use crate::test_support::Must;
use crate::workbench::{
    ActivationValues, LayoutNode, OverlayKind, PanelDescriptor, PanelId, PanelTypeId,
    PluginScreenId, RouteId, ScreenDescriptor, ScreenId, ScreenIdentity, ScreenRegistry,
    builtin_screens, intern,
};

use super::navigation::{
    Activation, NavIntent, NavMessage, NavOutcome, NavState, reduce_navigation,
};
use super::screen_overlays::ActiveOverlay;

/// The stable identity string of the composed package screen under test.
const PACKAGE_SCREEN: &str = "vendor.pkg.review";
/// The route the composed package screen is reachable through.
const PACKAGE_ROUTE: &str = "pkg-review";
/// The panel the package screen focuses on entry.
const PACKAGE_FOCUS_PANEL: &str = "list";

/// A registry containing every compiled screen plus one composed Package
/// screen, built exactly the way composition builds it: one
/// [`ScreenDescriptor`] whose identity is a [`ScreenIdentity::Package`].
fn registry_with_package() -> ScreenRegistry {
    let descriptor = package_descriptor();
    let mut screens = builtin_screens()
        .must("the compiled screen table is well formed")
        .screens()
        .to_vec();
    screens.push(descriptor);
    ScreenRegistry::new(screens).must("the composed registry is well formed")
}

/// Build the package screen descriptor used by every test.
fn package_descriptor() -> ScreenDescriptor {
    let id = intern(PACKAGE_SCREEN).must("interning a test identifier must succeed");
    let route = RouteId::parse(intern(PACKAGE_ROUTE).must("intern must succeed"))
        .must("the route identifier satisfies the grammar");
    let panel = PanelId::parse(intern(PACKAGE_FOCUS_PANEL).must("intern must succeed"))
        .must("the panel identifier satisfies the grammar");
    let panel_type = PanelTypeId::parse(intern("vendor.pkg.list").must("intern must succeed"))
        .must("the panel-type identifier satisfies the grammar");
    let identity = ScreenIdentity::Package(
        PluginScreenId::parse(id).must("the plugin screen identifier satisfies the grammar"),
    );
    ScreenDescriptor {
        id: identity,
        title: "Pkg Review".to_owned(),
        route,
        panels: vec![PanelDescriptor {
            id: panel,
            panel_type,
            host_capability: None,
            config: crate::domain::TypedMap::new(),
            focusable: true,
            required: true,
            ports: Vec::new(),
        }],
        initial_focus: panel,
        focus_order: vec![panel],
        layout: LayoutNode::Leaf { panel },
        relationships: Vec::new(),
        activation: Vec::new(),
        overlays: OverlayKind::ALL.to_vec(),
        host_capabilities: Vec::new(),
        bindings: Vec::new(),
    }
}

/// The composed package screen's identity.
fn package_identity() -> ScreenIdentity {
    let id = intern(PACKAGE_SCREEN).must("intern must succeed");
    ScreenIdentity::Package(
        PluginScreenId::parse(id).must("the plugin screen identifier satisfies the grammar"),
    )
}

/// The composed package screen's route.
fn package_route() -> RouteId {
    RouteId::parse(intern(PACKAGE_ROUTE).must("intern must succeed"))
        .must("the route identifier satisfies the grammar")
}

/// An activation targeting the package screen, computed from `state`'s live
/// instance exactly the way a real request would be.
fn package_request(state: &NavState) -> Activation {
    Activation::from_source(package_route(), ActivationValues::empty(), state.current())
}

// ── Push ────────────────────────────────────────────────────────────────────

#[test]
fn push_to_a_package_screen_commits_and_enters_it() {
    let registry = registry_with_package();
    let before = NavState::default();
    let suspended = before.current().id;
    let activation = package_request(&before);

    let transition = reduce_navigation(
        before,
        &registry,
        NavMessage::Navigate(NavIntent::Push(activation)),
    );

    assert!(
        matches!(transition.outcome, NavOutcome::Pushed { .. }),
        "push to a composed package screen must commit, got: {:?}",
        transition.outcome,
    );
    assert_eq!(
        transition.state.current().screen.as_str(),
        PACKAGE_SCREEN,
        "the entered instance must be the package screen"
    );
    assert_eq!(
        transition.state.depth(),
        1,
        "push must suspend the prior instance"
    );
    let NavOutcome::Pushed {
        suspended: reported_suspended,
        entered,
    } = transition.outcome
    else {
        unreachable!("the outcome assertion above already proved this is a push");
    };
    assert_eq!(reported_suspended, suspended);
    assert_eq!(entered, transition.state.current().id);
}

#[test]
fn a_pushed_package_instance_starts_at_the_descriptors_initial_focus() {
    let registry = registry_with_package();
    let descriptor = registry
        .get_identity(package_identity())
        .must("the package screen is in the composed registry");
    let before = NavState::default();
    let activation = package_request(&before);

    let transition = reduce_navigation(
        before,
        &registry,
        NavMessage::Navigate(NavIntent::Push(activation)),
    );

    assert_eq!(
        transition.state.current().panel_focus,
        descriptor.initial_focus,
        "a package screen must focus where its descriptor says, not where the compiled table assumes"
    );
}

// ── Replace ─────────────────────────────────────────────────────────────────

#[test]
fn replace_to_a_package_screen_disposes_the_current_without_stacking() {
    let registry = registry_with_package();
    let before = NavState::default();
    let disposed = before.current().id;
    let activation = package_request(&before);

    let transition = reduce_navigation(
        before,
        &registry,
        NavMessage::Navigate(NavIntent::Replace(activation)),
    );

    assert!(
        matches!(transition.outcome, NavOutcome::Replaced { .. }),
        "replace to a composed package screen must commit, got: {:?}",
        transition.outcome,
    );
    assert_eq!(transition.state.depth(), 0, "replace never grows the stack");
    assert_eq!(transition.state.current().screen.as_str(), PACKAGE_SCREEN,);
    let NavOutcome::Replaced {
        disposed: reported_disposed,
        ..
    } = transition.outcome
    else {
        unreachable!("the outcome assertion above already proved this is a replace");
    };
    assert_eq!(reported_disposed, disposed);
}

// ── Back ────────────────────────────────────────────────────────────────────

#[test]
fn back_from_a_package_screen_restores_the_exact_prior_instance() {
    let registry = registry_with_package();
    let root = NavState::default();
    let original = root.current().clone();

    let pushed = reduce_navigation(
        root.clone(),
        &registry,
        NavMessage::Navigate(NavIntent::Push(package_request(&root))),
    );
    assert!(
        matches!(pushed.outcome, NavOutcome::Pushed { .. }),
        "the setup push must commit before back is exercised: {:?}",
        pushed.outcome,
    );
    let disposed = pushed.state.current().id;

    let back = reduce_navigation(
        pushed.state,
        &registry,
        NavMessage::Navigate(NavIntent::Back),
    );

    assert!(
        matches!(back.outcome, NavOutcome::Restored { .. }),
        "back over a package screen must restore the prior instance: {:?}",
        back.outcome,
    );
    assert_eq!(back.state.current(), &original);
    assert_eq!(back.state.depth(), 0);
    let NavOutcome::Restored {
        disposed: reported_disposed,
        restored,
    } = back.outcome
    else {
        unreachable!("the outcome assertion above already proved this is a restore");
    };
    assert_eq!(reported_disposed, disposed);
    assert_eq!(restored, original.id);
}

// ── Compiled navigation remains unchanged ───────────────────────────────────

#[test]
fn suspended_and_active_screens_retain_independent_overlay_state() {
    let registry = registry_with_package();
    let mut root = NavState::default();
    let dashboard = registry
        .get_identity(root.screen())
        .unwrap_or_else(|| panic!("dashboard descriptor must exist"));
    assert!(root.ensure_current_relationships(dashboard).is_ok());
    assert!(root.current_mut().overlays_mut().open_search());
    assert!(
        root.current_mut()
            .overlays_mut()
            .replace_search("repositories".to_owned(), 12)
    );

    let activation = package_request(&root);
    let mut pushed = reduce_navigation(
        root,
        &registry,
        NavMessage::Navigate(NavIntent::Push(activation)),
    );
    assert!(matches!(pushed.outcome, NavOutcome::Pushed { .. }));
    assert!(pushed.state.current_mut().overlays_mut().open_help());
    assert_eq!(
        pushed.state.current().overlays().active(),
        Some(&ActiveOverlay::Help { viewport: 0 })
    );

    let restored = reduce_navigation(
        pushed.state,
        &registry,
        NavMessage::Navigate(NavIntent::Back),
    );
    assert_eq!(
        restored.state.current().overlays().active(),
        Some(&ActiveOverlay::Search {
            query: "repositories".to_owned(),
            cursor: 12,
        })
    );
}

#[test]
fn compiled_push_is_unchanged_when_a_package_screen_is_composed() {
    let registry = registry_with_package();
    let before = NavState::default();

    let route = registry
        .get(ScreenId::Issues)
        .must("every compiled screen is in the composed registry")
        .route;
    let activation = Activation::from_source(route, ActivationValues::empty(), before.current());
    let transition = reduce_navigation(
        before,
        &registry,
        NavMessage::Navigate(NavIntent::Push(activation)),
    );

    assert!(
        matches!(transition.outcome, NavOutcome::Pushed { .. }),
        "compiled navigation must still commit"
    );
    assert_eq!(transition.state.current().screen.as_str(), "github.issues");
    assert_eq!(transition.state.depth(), 1);
}
