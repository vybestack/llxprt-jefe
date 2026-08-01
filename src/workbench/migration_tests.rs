//! One-way legacy screen-value migration matrix (issue #384, CW04-09).

use super::migration::{LEGACY_SCREEN_VALUES, MigrationOutcome, migrate_legacy_screen_value};
use super::screens::{ScreenRegistry, builtin_screens};

fn registry() -> ScreenRegistry {
    builtin_screens().unwrap_or_else(|error| unreachable!("compiled screens are valid: {error}"))
}

#[test]
fn every_legacy_value_maps_to_its_specified_stable_identity() {
    let registry = registry();
    let expected = [
        ("Dashboard", "core.dashboard"),
        ("Split", "core.repositories"),
        ("DashboardIssues", "github.issues"),
        ("DashboardPullRequests", "github.pull-requests"),
        ("DashboardActions", "github.actions"),
    ];
    for (legacy, stable) in expected {
        let outcome = migrate_legacy_screen_value(Some(legacy), &registry);
        assert!(
            matches!(&outcome, Some(MigrationOutcome::Mapped(id)) if id.as_str() == stable),
            "{legacy} must map to {stable}, got {outcome:?}"
        );
    }
}

#[test]
fn the_migration_table_lists_each_legacy_value_exactly_once() {
    for (index, (legacy, _)) in LEGACY_SCREEN_VALUES.iter().enumerate() {
        assert!(
            !LEGACY_SCREEN_VALUES[..index]
                .iter()
                .any(|(prior, _)| prior == legacy),
            "legacy value {legacy} is listed twice"
        );
    }
}

#[test]
fn the_migration_table_covers_every_registered_screen_exactly_once() {
    let registry = registry();
    let mut targets: Vec<&str> = LEGACY_SCREEN_VALUES
        .iter()
        .map(|(_, stable)| *stable)
        .collect();
    targets.sort_unstable();
    let mut registered: Vec<&str> = registry
        .screens()
        .iter()
        .map(|screen| screen.id.as_str())
        .collect();
    registered.sort_unstable();
    assert_eq!(targets, registered);
}

#[test]
fn an_unrecognised_legacy_value_falls_back_to_the_initial_screen() {
    let registry = registry();
    let outcome = migrate_legacy_screen_value(Some("DashboardMystery"), &registry);
    assert!(
        matches!(
            &outcome,
            Some(MigrationOutcome::FellBackToInitial(id)) if id.as_str() == "core.dashboard"
        ),
        "an unknown value must fall back to the compiled initial screen, got {outcome:?}"
    );
}

#[test]
fn a_missing_legacy_value_falls_back_to_the_initial_screen() {
    let registry = registry();
    let outcome = migrate_legacy_screen_value(None, &registry);
    assert!(
        matches!(
            &outcome,
            Some(MigrationOutcome::FellBackToInitial(id)) if id.as_str() == "core.dashboard"
        ),
        "an absent value must fall back to the compiled initial screen, got {outcome:?}"
    );
}

#[test]
fn migration_never_depends_on_the_position_of_a_legacy_value() {
    // Selecting by ordinal rather than by name would map the third entry to the
    // third screen; this asserts the mapping is by name.
    let registry = registry();
    let outcome = migrate_legacy_screen_value(Some("Split"), &registry);
    assert!(
        matches!(&outcome, Some(MigrationOutcome::Mapped(id)) if id.as_str() == "core.repositories"),
        "Split must map by name to core.repositories, got {outcome:?}"
    );
}

#[test]
fn the_migrated_identity_is_always_present_in_the_registry() {
    let registry = registry();
    for (legacy, _) in LEGACY_SCREEN_VALUES {
        let Some(outcome) = migrate_legacy_screen_value(Some(legacy), &registry) else {
            unreachable!("the shipped registry is never empty");
        };
        assert!(
            registry.get(outcome.screen_id()).is_some(),
            "{legacy} migrated to an identity the registry does not contain"
        );
    }
}
