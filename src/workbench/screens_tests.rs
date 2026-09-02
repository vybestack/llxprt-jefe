//! Shipped-screen parity tests (issue #384, CW04-01).
//!
//! The compiled table is compared against a checked-in golden so a change to
//! any screen's identity, panel set, focus order, requiredness, or collapse
//! order has to be made deliberately in both places.

use serde_json::Value;

use super::descriptor::{HostPanelModelSource, LayoutChild, LayoutNode, ScreenDescriptor};
use super::geometry::Rect;
use super::ids::{DASHBOARD_IDENTITY, MAX_PANELS_PER_SCREEN, PanelId, ScreenId, ScreenInstanceId};
use super::resolve::{PanelState, resolve_layout};
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

/// Walk the tree, recording each collapsible child with the leaf index its
/// subtree starts at. The resolver breaks collapse-priority ties by that same
/// index, so this mirrors it rather than inventing an ordering.
fn collect_collapsible(
    node: &LayoutNode,
    next_leaf: &mut usize,
    collected: &mut Vec<(i32, usize, String)>,
) {
    match node {
        LayoutNode::Leaf { .. } => *next_leaf += 1,
        LayoutNode::Split { children, .. } => {
            for child in children {
                let index = *next_leaf;
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
                collect_collapsible(&child.node, next_leaf, collected);
            }
        }
    }
}

fn golden() -> Value {
    serde_json::from_str(PARITY_GOLDEN)
        .unwrap_or_else(|error| unreachable!("parity golden is valid JSON: {error}"))
}

#[test]
fn the_registry_declares_the_five_parity_screens_with_their_stable_identities() {
    let registry = registry();
    let ids: Vec<&str> = registry
        .screens()
        .iter()
        .map(|screen| screen.id.as_str())
        .collect();
    for parity in [
        "core.dashboard",
        "core.repositories",
        "github.issues",
        "github.pull-requests",
        "github.actions",
    ] {
        assert!(ids.contains(&parity), "{parity} must be compiled in");
    }
}

#[test]
fn the_registry_covers_every_screen_the_application_can_display() {
    // The registry replaces the legacy screen enum outright, so every screen
    // that can be displayed needs a stable identity — not only the five that
    // carry explicit parity guarantees.
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
            "core.errors",
            "core.terminals",
            "core.settings",
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
fn every_declared_screen_constant_satisfies_the_identifier_grammar() {
    // The constants are declared with `from_static`, which cannot validate in a
    // const context, so this is where a malformed literal is caught.
    for id in ScreenId::ALL {
        assert_eq!(id.check(), Ok(()), "screen constant {id} is malformed");
    }
}

#[test]
fn the_compiled_residual_set_is_exactly_five_and_a_strict_subset_of_the_recorded_set() {
    let registry = registry();
    let expected = [
        ScreenId::Issues,
        ScreenId::PullRequests,
        ScreenId::Actions,
        ScreenId::Errors,
        ScreenId::Settings,
    ];
    let registered: Vec<ScreenId> = registry
        .screens()
        .iter()
        .filter_map(|screen| screen.id.compiled())
        .collect();
    let repositories = registry
        .screens()
        .iter()
        .find(|screen| screen.id.as_str() == crate::workbench::REPOSITORIES_SCREEN_ID.as_str());

    assert_eq!(ScreenId::ALL.as_slice(), expected.as_slice());
    assert_eq!(registered.as_slice(), expected.as_slice());
    assert!(
        repositories.is_some_and(|screen| screen.id.compiled().is_none()),
        "Repositories must be an open shared-runtime definition, not a residual compiled adapter"
    );

    // The recorded residual set is what the issue #705 owner-evidence ledger
    // shipped: compiled adapters including Repositories. The #706 cutover
    // migrates exactly one of them onto the shared runtime, so the live set
    // must be a strict subset whose only missing element is the repositories
    // builtin — no silent additions, no second migration hiding in the diff.
    let recorded = [
        "github.issues",
        "github.pull-requests",
        "github.actions",
        "core.errors",
        "core.settings",
        "core.repositories",
    ];
    let live: Vec<&str> = ScreenId::ALL.iter().map(|id| id.as_str()).collect();
    for id in &live {
        assert!(
            recorded.contains(id),
            "screen {id} joined the residual set outside the recorded migration"
        );
    }
    let migrated: Vec<&str> = recorded
        .iter()
        .filter(|id| !live.contains(id))
        .copied()
        .collect();
    assert_eq!(
        migrated.as_slice(),
        [crate::workbench::REPOSITORIES_SCREEN_ID.as_str()],
        "exactly the repositories screen may have migrated off the recorded residual set"
    );
}

#[test]
fn an_unregistered_value_does_not_resolve_to_a_screen_identity() {
    let registry = registry();
    assert_eq!(registry.resolve("core.nonesuch"), None);
    assert_eq!(registry.resolve(""), None);
    assert_eq!(registry.resolve("core.dashboard"), Some(DASHBOARD_IDENTITY));
}

#[test]
fn the_first_screen_is_the_open_dashboard_definition() {
    let registry = registry();
    assert_eq!(
        registry.initial_screen().map(|screen| screen.id.as_str()),
        Some("core.dashboard")
    );
}

#[test]
fn dashboard_declares_its_footer_context_from_the_canonical_action_inventory() {
    let registry = registry();
    let Some(dashboard) = registry.get_identity(DASHBOARD_IDENTITY) else {
        panic!("Dashboard descriptor must be published");
    };
    let Ok(inventory) = crate::domain::default_action_inventory::compiled_inventory() else {
        panic!("compiled action inventory must be valid");
    };
    let Some(help_action) = inventory.actions.iter().find(|action| {
        action.handler == crate::domain::action_registry::HandlerKey::OpenHelp
            && action
                .contexts
                .iter()
                .any(|context| context.as_str() == "dashboard")
    }) else {
        panic!("canonical Dashboard Help action must exist");
    };

    assert_eq!(dashboard.bindings.len(), 1);
    assert_eq!(dashboard.bindings[0].context.as_str(), "dashboard");
    assert_eq!(dashboard.bindings[0].action, help_action.id);
}

#[test]
fn lookup_by_stable_identity_finds_each_screen() {
    let registry = registry();
    for screen in registry.screens() {
        let looked_up = registry
            .resolve(screen.id.as_str())
            .and_then(|id| registry.get_identity(id));
        assert_eq!(looked_up.map(|found| &found.id), Some(&screen.id));
    }
}

#[test]
fn exactly_the_screens_that_host_a_live_terminal_declare_a_pty_panel() {
    for screen in registry().screens() {
        let pty_panels: Vec<&str> = screen
            .panels
            .iter()
            .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
            .map(|panel| panel.id.as_str())
            .collect();
        let expected: Vec<&str> = match screen.id.as_str() {
            "core.dashboard" => vec!["terminal"],
            "core.terminals" => vec!["shell-preview"],
            _ => Vec::new(),
        };
        assert_eq!(pty_panels, expected, "screen {}", screen.id);
    }
}

#[test]
fn every_workspace_screen_shares_the_repository_sidebar() {
    for screen in registry().screens() {
        // The split view *is* the repository list, and Settings edits the
        // configuration document rather than anything a repository owns, so
        // neither carries the sidebar.
        if matches!(screen.id.as_str(), "core.repositories" | "core.settings") {
            continue;
        }
        assert!(
            screen
                .panels
                .iter()
                .any(|panel| panel.id.as_str() == "repositories" && panel.focusable),
            "screen {} must declare the focusable repository sidebar it renders",
            screen.id
        );
    }
}

#[test]
fn a_screen_opens_on_its_declared_initial_focus_not_the_head_of_its_focus_order() {
    // The workspace screens cycle through the sidebar but open on their list,
    // so initial focus and focus-order head are genuinely different values.
    for screen in registry().screens() {
        assert!(
            screen
                .focus_order
                .iter()
                .any(|panel| panel == &screen.initial_focus),
            "screen {} opens on a panel outside its focus order",
            screen.id
        );
    }
    let registry = registry();
    let Some(issues) = registry
        .screens()
        .iter()
        .find(|screen| screen.id.as_str() == "github.issues")
    else {
        unreachable!("the issues screen is compiled in");
    };
    assert_eq!(issues.initial_focus.as_str(), "issue-list");
    assert_eq!(
        issues.focus_order.first().copied().map(PanelId::as_str),
        Some("repositories")
    );
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

        let focus: Vec<&str> = screen
            .focus_order
            .iter()
            .copied()
            .map(PanelId::as_str)
            .collect();
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

/// Read a list of strings from the golden, failing loudly on schema drift.
///
/// Silently dropping a non-string would let a corrupted golden compare against
/// a truncated list and pass.
fn string_list<'value>(value: &'value Value, key: &str) -> Option<Vec<&'value str>> {
    let entries = value.get(key).and_then(Value::as_array)?;
    let strings: Vec<&str> = entries.iter().filter_map(Value::as_str).collect();
    assert_eq!(
        strings.len(),
        entries.len(),
        "golden key {key} contains a non-string entry"
    );
    Some(strings)
}

#[test]
fn every_layout_child_declares_a_coherent_size_range() {
    for screen in registry().screens() {
        assert_children(&screen.layout, &mut |child| {
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

#[test]
fn no_flexible_child_reserves_a_minimum_it_does_not_need() {
    // A minimum is charged before weights apply, so declaring one skews the
    // proportion a pane actually receives. The shipped screens are pure
    // proportions, so only fixed-width children carry a minimum.
    for screen in registry().screens() {
        assert_children(&screen.layout, &mut |child| {
            if matches!(child.size, super::descriptor::Size::Weight(_)) {
                assert_eq!(
                    child.min, 0,
                    "screen {} reserves a minimum on a weighted child, which skews its share",
                    screen.id
                );
            }
        });
    }
}

#[test]
fn shipped_builtin_count_matches_the_declared_table() {
    let builtins = registry()
        .screens()
        .iter()
        .filter(|screen| screen.id.compiled().is_none())
        .count();
    assert_eq!(
        crate::workbench::screens::SHIPPED_BUILTIN_SCREENS,
        3,
        "the shipped builtins are exactly dashboard, terminals, and repositories"
    );
    assert_eq!(builtins, 3, "three shipped screens are builtin definitions");
    assert_eq!(
        builtins,
        crate::workbench::screens::SHIPPED_BUILTIN_SCREENS,
        "SHIPPED_BUILTIN_SCREENS must name exactly the builtin descriptor screens"
    );
    assert_eq!(
        registry().screens().len(),
        ScreenId::ALL.len() + crate::workbench::screens::SHIPPED_BUILTIN_SCREENS,
        "the shipped table is the residual compiled screens plus the builtins"
    );
}

#[test]
fn initial_focus_agrees_with_every_descriptor() {
    // The compiled table lets a screen instance be created without a fallible
    // registry lookup, which is only safe while the two agree exactly.
    for screen in super::ids::ScreenId::ALL {
        let compiled = registry();
        let Some(descriptor) = compiled.get(screen) else {
            panic!("every compiled screen has a descriptor");
        };
        assert_eq!(
            super::screens::initial_focus(screen),
            descriptor.initial_focus,
            "the compiled initial focus for {screen} has drifted from its descriptor"
        );
    }
}

#[test]
fn route_agrees_with_every_descriptor() {
    for screen in super::ids::ScreenId::ALL {
        let compiled = registry();
        let Some(descriptor) = compiled.get(screen) else {
            panic!("every compiled screen has a descriptor");
        };
        assert_eq!(
            super::screens::route_of(screen),
            descriptor.route,
            "the compiled route for {screen} has drifted from its descriptor"
        );
        assert_eq!(super::screens::route_of(screen).check(), Ok(()));
    }
}

#[test]
fn every_compiled_initial_focus_satisfies_the_identifier_grammar() {
    // `initial_focus` builds its panel ids with the unchecked const
    // constructor, so the grammar is asserted here instead.
    for screen in super::ids::ScreenId::ALL {
        assert_eq!(
            super::screens::initial_focus(screen).check(),
            Ok(()),
            "the compiled initial focus for {screen} is not a valid identifier"
        );
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

#[test]
fn mutating_a_legacy_product_spelling_does_not_change_compiled_host_authority() {
    let mut dashboard = registry()
        .get_identity(DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("compiled dashboard"))
        .clone();
    let repository = dashboard
        .panels
        .iter_mut()
        .find(|panel| panel.id.as_str() == "repositories")
        .unwrap_or_else(|| panic!("repository panel"));
    let authority = repository
        .host_capability()
        .unwrap_or_else(|| panic!("compiled repository authority"));

    repository.panel_type = super::ids::PanelTypeId::from_static("notice-band");

    assert_eq!(repository.host_capability(), Some(authority));
    assert_eq!(
        authority.model_source(),
        super::descriptor::HostPanelModelSource::RepositoryList
    );
    assert_eq!(
        authority.control_kind(),
        crate::host_controls::ControlKind::List
    );
}

#[test]
fn repositories_cards_grid_is_a_declared_host_control() {
    let registry = registry();
    let descriptor = registry
        .get_identity(crate::workbench::REPOSITORIES_IDENTITY)
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let cards_grid = descriptor
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "cards")
        .unwrap_or_else(|| panic!("cards panel must be declared"));
    let capability = cards_grid
        .host_capability()
        .unwrap_or_else(|| unreachable!("cards must be a declared host control"));
    assert_eq!(
        capability.model_source(),
        super::descriptor::HostPanelModelSource::WorkbenchCards
    );
    assert_eq!(
        capability.control_kind(),
        crate::host_controls::ControlKind::List
    );
}

#[test]
fn terminals_shell_list_is_a_declared_host_control() {
    let registry = registry();
    let descriptor = registry
        .get_identity(crate::workbench::TERMINALS_IDENTITY)
        .unwrap_or_else(|| panic!("terminals descriptor must be published"));
    let shell_list = descriptor
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "shell-list")
        .unwrap_or_else(|| panic!("shell-list panel must be declared"));
    let capability = shell_list
        .host_capability()
        .unwrap_or_else(|| unreachable!("shell-list must be a declared host control"));
    assert_eq!(
        capability.model_source(),
        super::descriptor::HostPanelModelSource::SessionList
    );
    assert_eq!(
        capability.control_kind(),
        crate::host_controls::ControlKind::List
    );
}

#[test]
fn repositories_status_block_is_a_declared_host_control() {
    let registry = registry();
    let descriptor = registry
        .get_identity(crate::workbench::REPOSITORIES_IDENTITY)
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let status_block = descriptor
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "status")
        .unwrap_or_else(|| panic!("status panel must be declared"));
    let capability = status_block
        .host_capability()
        .unwrap_or_else(|| unreachable!("status must be a declared host control"));
    assert_eq!(
        capability.model_source(),
        super::descriptor::HostPanelModelSource::WorkbenchStatus
    );
    assert_eq!(
        capability.control_kind(),
        crate::host_controls::ControlKind::List
    );
}

/// The STATUS pane must fit every bucket row the projection emits.
///
/// The pane draws its own border and title (`LIST_PANE_CHROME`), so the fixed
/// claim has to cover that chrome plus one row per bucket. The scenario
/// manifest `workbench-screen.json` asserts the same four labels against the
/// real process; this pins the geometry and the clipped projection that
/// produce them, at the scenario's terminal size.
#[test]
fn repositories_status_pane_fits_every_bucket_row_at_120x36() {
    let registry = registry();
    let descriptor = registry
        .get_identity(crate::workbench::REPOSITORIES_IDENTITY)
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let layout = resolve_layout(
        descriptor,
        ScreenInstanceId::next(),
        Rect::new(0, 0, 120, 36),
        &PanelState::all_visible(),
    )
    .unwrap_or_else(|error| panic!("resolution must not fail: {error}"));
    assert!(
        layout.too_small.is_none(),
        "the repositories screen must fit at 120x36"
    );

    let status = layout
        .panel(&PanelId::from_static("status"))
        .unwrap_or_else(|| panic!("status panel must resolve"));
    assert!(status.visible, "the status panel must be visible at 120x36");
    let buckets = crate::workbench_view::STATUS_BLOCK_ORDER.len();
    assert!(
        usize::from(status.content.height) >= buckets,
        "status interior is {} rows but must fit all {buckets} bucket rows",
        status.content.height
    );

    // Every row the projection emits, clipped to the interior height the pane
    // renders — the same clip `project_host_model` applies before drawing.
    let state = crate::state::AppState::test_fixture();
    let model =
        crate::host_panel_models::project_host_panel(&state, HostPanelModelSource::WorkbenchStatus);
    let rows = crate::host_controls::project_control_body(
        &model.body,
        &model.action_affordances,
        model.selected_id.as_ref(),
        None,
        usize::from(status.content.width),
    );
    let visible: Vec<&str> = rows
        .iter()
        .take(usize::from(status.content.height))
        .map(|row| row.text.as_str())
        .collect();
    for label in ["Needs you", "Working", "Ready", "Stale"] {
        assert!(
            visible.iter().any(|text| text.contains(label)),
            "status interior must show the {label} bucket; visible rows: {visible:?}"
        );
    }

    // Growing the status claim must not push the repository list out of the
    // rows it renders at this size (its empty-state line plus one entry).
    let sidebar = layout
        .panel(&PanelId::from_static("repositories"))
        .unwrap_or_else(|| panic!("repositories panel must resolve"));
    assert!(
        sidebar.visible && usize::from(sidebar.content.height) >= 2,
        "repository list must keep at least its two rendered rows, has {}",
        sidebar.content.height
    );
}
