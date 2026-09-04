//! Behavioral tests for the Screens/Layout editor projection (issue #388).
//!
//! @requirement CW08-02
//! @requirement CW08-03
//! @requirement CW08-04

use std::num::NonZeroU16;

use crate::domain::action_registry::Provenance;
use crate::persistence::settings_document::PublishedSettings;
use crate::workbench::descriptor::{Axis, LayoutChild, LayoutNode, Size};
use crate::workbench::ids::PanelId;
use crate::workbench::screens::{ScreenRegistry, builtin_screens};

use super::{
    CompositionStatus, ScreenEditorRow, preview_layout, project_screens, screen_membership,
};

fn registry() -> ScreenRegistry {
    builtin_screens().unwrap_or_else(|error| panic!("compiled screen table: {error}"))
}

fn published(source: &str) -> PublishedSettings {
    let catalog = crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    crate::persistence::migration::migrate_settings(source.as_bytes(), &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture: {diagnostics:?}"))
        .published()
        .clone()
}

fn row<'rows>(rows: &'rows [ScreenEditorRow], id: &str) -> &'rows ScreenEditorRow {
    rows.iter()
        .find(|row| row.screen_id.as_str() == id)
        .unwrap_or_else(|| panic!("row for {id}"))
}

fn panel(value: &'static str) -> PanelId {
    PanelId::parse(value).unwrap_or_else(|error| panic!("panel fixture {value}: {error}"))
}

fn weight(value: u16) -> Size {
    Size::Weight(NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN))
}

// ── CW08-02: rows, order, and the membership the editor serializes ────────

#[test]
fn every_registered_screen_projects_exactly_one_row() {
    let registry = registry();

    let rows = project_screens(&registry, &PublishedSettings::default());

    assert_eq!(rows.len(), registry.screens().len());
    for screen in registry.screens() {
        let row = row(&rows, screen.id.as_str());
        assert_eq!(row.title, screen.title);
    }
}

/// Issue #742: the editor lists screens, so every row is a screen name. The
/// composition root's row is the one at risk, because the top band brands the
/// product from the same registry; if the two ever share a string again this
/// row reads `LLxprt Jefe` in a list of screens.
#[test]
fn composition_root_row_is_named_for_the_screen_not_the_application() {
    let registry = registry();

    let rows = project_screens(&registry, &PublishedSettings::default());

    let root = row(&rows, crate::workbench::DASHBOARD_IDENTITY.as_str());
    assert_eq!(root.title, "Dashboard");
    for row in &rows {
        assert_ne!(
            row.title,
            crate::PRODUCT_NAME,
            "screen {} is listed under the application name",
            row.screen_id.as_str()
        );
    }
}

#[test]
fn rows_follow_the_documents_order_and_then_the_registrys_own() {
    let registry = registry();
    let first = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let last = registry
        .screens()
        .last()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let published = published(&format!(
        "settings_schema = 2\n[workbench]\nscreen_order = [\"{}\"]\n",
        last.id.as_str()
    ));

    let rows = project_screens(&registry, &published);

    assert_eq!(
        rows.first().map(|row| row.screen_id.as_str()),
        Some(last.id.as_str()),
        "an ordered screen leads"
    );
    assert!(
        rows.iter()
            .any(|row| row.screen_id.as_str() == first.id.as_str()),
        "a screen the order omits still has a row"
    );
    let indexes: Vec<_> = rows.iter().map(|row| row.order_index).collect();
    let expected: Vec<u16> = (0..u16::try_from(rows.len()).unwrap_or(u16::MAX)).collect();
    assert_eq!(indexes, expected, "order indexes are the row positions");
}

#[test]
fn every_mandatory_shipped_screen_is_enabled_and_locked() {
    let registry = registry();

    let rows = project_screens(&registry, &PublishedSettings::default());

    for row in &rows {
        assert!(row.enabled, "{} is shipped", row.screen_id.as_str());
        assert!(
            row.enablement_locked.is_some(),
            "a mandatory shipped screen cannot be disabled, and says so"
        );
    }
}

#[test]
fn membership_lists_every_enabled_screen_exactly_once_and_no_disabled_one() {
    let registry = registry();
    let rows = project_screens(&registry, &PublishedSettings::default());

    let membership = screen_membership(&rows);

    assert_eq!(
        membership.len(),
        rows.iter().filter(|row| row.enabled).count(),
        "every enabled screen appears"
    );
    let mut sorted = membership.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), membership.len(), "and appears exactly once");
    for row in rows.iter().filter(|row| !row.enabled) {
        assert!(
            !membership
                .iter()
                .any(|id| id.as_str() == row.screen_id.as_str()),
            "a disabled screen is absent"
        );
    }
}

#[test]
fn a_screen_the_document_orders_reports_that_provenance() {
    let registry = registry();
    let ordered = registry
        .screens()
        .last()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let published = published(&format!(
        "settings_schema = 2\n[workbench]\nscreen_order = [\"{}\"]\n",
        ordered.id.as_str()
    ));

    let rows = project_screens(&registry, &published);

    assert!(matches!(
        row(&rows, ordered.id.as_str()).provenance,
        Provenance::Settings { .. }
    ));
    let untouched = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    assert_eq!(
        row(&rows, untouched.id.as_str()).provenance,
        Provenance::Compiled
    );
}

// ── CW08-03/04: composition status comes from the descriptor validator ────

#[test]
fn a_screen_with_no_override_composes_as_the_registry_declares_it() {
    let registry = registry();

    let rows = project_screens(&registry, &PublishedSettings::default());

    for row in &rows {
        assert_eq!(
            row.composition,
            CompositionStatus::Valid,
            "{} is compiled in and already validated",
            row.screen_id.as_str()
        );
    }
}

#[test]
fn an_override_placing_an_undeclared_panel_reports_the_validators_own_refusal() {
    let registry = registry();
    let screen = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let published = published(&format!(
        "settings_schema = 2\n[workbench.layout_overrides]\n\"{}\" = {{ type = \"leaf\", panel = \"nothing-declares-me\" }}\n",
        screen.id.as_str()
    ));

    let rows = project_screens(&registry, &published);

    let CompositionStatus::Invalid { code, reason } = &row(&rows, screen.id.as_str()).composition
    else {
        panic!("an override the descriptor validator refuses is invalid");
    };
    assert_eq!(code, "SCR-E301");
    assert!(
        reason.contains("nothing-declares-me") || reason.contains("panel"),
        "the reason is the validator's own: {reason}"
    );
}

#[test]
fn a_valid_override_replaces_the_layout_the_preview_resolves() {
    let registry = registry();
    let screen = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));
    let only = screen
        .panels
        .first()
        .unwrap_or_else(|| panic!("a declared panel"));

    let preview = preview_layout(screen, &LayoutNode::Leaf { panel: only.id }, 100, 24)
        .unwrap_or_else(|error| panic!("a one-panel override resolves: {error:?}"));

    assert!(
        preview
            .panel(&only.id)
            .is_some_and(|resolved| resolved.visible),
        "the previewed panel occupies the whole rectangle"
    );
}

#[test]
fn a_preview_at_small_dimensions_still_answers_rather_than_failing() {
    let registry = registry();
    let screen = registry
        .screens()
        .first()
        .unwrap_or_else(|| panic!("a compiled screen"));

    let preview = preview_layout(screen, &screen.layout, 16, 6)
        .unwrap_or_else(|error| panic!("the resolver answers at any size: {error:?}"));

    assert!(
        preview.visible_panels().count() >= 1,
        "a too-small screen still shows its required panel"
    );
}

#[test]
fn a_split_override_previews_both_children() {
    let registry = registry();
    let screen = registry
        .screens()
        .iter()
        .find(|screen| screen.panels.len() >= 2)
        .unwrap_or_else(|| panic!("a screen with two panels"));
    let left = screen.panels[0].id;
    let right = screen.panels[1].id;

    let preview = preview_layout(
        screen,
        &LayoutNode::Split {
            axis: Axis::Horizontal,
            gap: 0,
            children: vec![
                LayoutChild {
                    node: LayoutNode::Leaf { panel: left },
                    size: weight(1),
                    min: 10,
                    max: None,
                    collapsible: false,
                    collapse_priority: None,
                },
                LayoutChild {
                    node: LayoutNode::Leaf { panel: right },
                    size: weight(1),
                    min: 10,
                    max: None,
                    collapsible: false,
                    collapse_priority: None,
                },
            ],
        },
        100,
        24,
    )
    .unwrap_or_else(|error| panic!("a two-panel split resolves: {error:?}"));

    assert!(
        preview
            .panel(&left)
            .is_some_and(|resolved| resolved.visible)
    );
    assert!(
        preview
            .panel(&right)
            .is_some_and(|resolved| resolved.visible)
    );
    let _ = panel("list");
}

#[test]
fn projecting_the_same_registry_and_document_twice_produces_the_same_rows() {
    let registry = registry();
    let published = published("settings_schema = 2\n");

    assert_eq!(
        project_screens(&registry, &published),
        project_screens(&registry, &published),
        "the projection is a pure function of its inputs"
    );
}

#[test]
fn an_override_naming_a_declared_panel_but_dropping_the_others_is_refused() {
    let registry = registry();
    let screen = registry
        .screens()
        .iter()
        .find(|screen| screen.id.as_str() == "core.dashboard")
        .unwrap_or_else(|| panic!("the dashboard is compiled in"));
    let published = published(concat!(
        "settings_schema = 2\n",
        "[workbench.layout_overrides]\n",
        "\"core.dashboard\" = { type = \"leaf\", panel = \"search\" }\n",
    ));

    assert!(
        !published.workbench.layout_overrides.is_empty(),
        "the override publishes: {:?}",
        published.workbench
    );

    let rows = project_screens(&registry, &published);
    let row = row(&rows, screen.id.as_str());
    assert!(
        matches!(row.composition, CompositionStatus::Invalid { .. }),
        "an override leaving declared panels unplaced is refused, got {:?}",
        row.composition
    );
}

#[test]
fn an_override_declaring_an_unknown_field_is_refused_by_the_layout_grammar() {
    let registry = registry();
    let published = published(concat!(
        "settings_schema = 2\n",
        "[workbench.layout_overrides]\n",
        "\"core.dashboard\" = { type = \"leaf\", panel = \"list\", nonsense = 1 }\n",
    ));

    let rows = project_screens(&registry, &published);

    assert!(
        matches!(
            row(&rows, "core.dashboard").composition,
            CompositionStatus::Invalid { .. }
        ),
        "the definition grammar refuses unknown fields, and so must an override"
    );
}

#[test]
fn an_override_declaring_both_size_variants_is_refused() {
    let registry = registry();
    let published = published(concat!(
        "settings_schema = 2\n",
        "[workbench.layout_overrides]\n",
        "\"core.dashboard\" = { type = \"split\", axis = \"horizontal\", children = [",
        "{ node = { type = \"leaf\", panel = \"list\" }, size = { fixed = 10, weight = 1 }, ",
        "min = 1, collapsible = false }] }\n",
    ));

    let rows = project_screens(&registry, &published);

    assert!(
        matches!(
            row(&rows, "core.dashboard").composition,
            CompositionStatus::Invalid { .. }
        ),
        "a child claims cells one way, and declaring both is not a preference"
    );
}
