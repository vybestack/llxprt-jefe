//! Plugins section projection table (issue #389 CW-09, acceptance rows U1–U3).

use super::*;

fn row(id: &str, version: &str, state: PluginRowState) -> PluginSnapshotRow {
    PluginSnapshotRow {
        id: id.to_owned(),
        display_name: "Git Merger".to_owned(),
        version: version.to_owned(),
        versions: vec![version.to_owned()],
        root: "user".to_owned(),
        state,
    }
}

fn none_trusted(_id: &str) -> bool {
    false
}

fn all_trusted(_id: &str) -> bool {
    true
}

fn project(snapshot: &[PluginSnapshotRow], trusted: &dyn Fn(&str) -> bool) -> Vec<PluginRow> {
    project_plugins(snapshot, trusted)
}

#[test]
fn an_installed_package_renders_its_name_version_and_trust() {
    let rows = project(
        &[row("vendor.git-merger", "1.0.0", PluginRowState::Installed)],
        &none_trusted,
    );
    let first = rows.first().unwrap_or_else(|| panic!("one row"));
    assert_eq!(first.label, "Git Merger 1.0.0");
    assert!(!first.enabled);
    assert_eq!(first.status, "installed");
    assert_eq!(first.detail, None);
    assert_eq!(first.to_string(), "Git Merger 1.0.0 disabled installed");
}

#[test]
fn a_trusted_package_reports_enabled() {
    let rows = project(
        &[row("vendor.git-merger", "1.0.0", PluginRowState::Installed)],
        &all_trusted,
    );
    let first = rows.first().unwrap_or_else(|| panic!("one row"));
    assert!(first.enabled);
    assert_eq!(first.to_string(), "Git Merger 1.0.0 enabled installed");
}

#[test]
fn an_unsupported_package_is_listed_with_the_host_it_lacks() {
    let rows = project(
        &[row(
            "vendor.git-merger",
            "2.0.0",
            PluginRowState::UnsupportedPlatform {
                reason: "no binary for aarch64-apple-darwin".to_owned(),
            },
        )],
        &none_trusted,
    );
    let first = rows.first().unwrap_or_else(|| panic!("one row"));
    assert_eq!(first.status, "Unsupported platform");
    assert_eq!(
        first.detail.as_deref(),
        Some("no binary for aarch64-apple-darwin")
    );
    assert!(
        first.selectable,
        "an unsupported package is still installed and may be trusted for another host"
    );
}

#[test]
fn an_ambiguous_package_shows_its_code_and_cannot_be_trusted() {
    let rows = project(
        &[row(
            "vendor.dup",
            "1.0.0",
            PluginRowState::Ambiguous {
                code: PluginCode::Ambiguous,
                paths: vec!["/a".to_owned(), "/b".to_owned()],
            },
        )],
        &all_trusted,
    );
    let first = rows.first().unwrap_or_else(|| panic!("one row"));
    assert_eq!(first.status, "Ambiguous PLG-E501");
    assert_eq!(first.detail.as_deref(), Some("2 physical package paths"));
    assert!(!first.selectable);
    assert!(
        !first.enabled,
        "a package that cannot be selected must never render as trusted"
    );
}

#[test]
fn an_unavailable_package_shows_why_and_cannot_be_trusted() {
    let rows = project(
        &[row(
            "vendor.broken",
            "1.0.0",
            PluginRowState::Unavailable {
                reason: "no plugin.json in the version directory".to_owned(),
            },
        )],
        &all_trusted,
    );
    let first = rows.first().unwrap_or_else(|| panic!("one row"));
    assert_eq!(first.status, "unavailable");
    assert_eq!(
        first.detail.as_deref(),
        Some("no plugin.json in the version directory")
    );
    assert!(!first.selectable);
    assert!(!first.enabled);
}

#[test]
fn a_broken_package_does_not_hide_a_valid_neighbour() {
    let rows = project(
        &[
            row(
                "vendor.broken",
                "1.0.0",
                PluginRowState::Unavailable {
                    reason: "schema 9".to_owned(),
                },
            ),
            row("vendor.good", "1.0.0", PluginRowState::Installed),
        ],
        &none_trusted,
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].status, "installed");
    assert!(rows[1].selectable);
}

#[test]
fn every_state_is_distinguishable_without_colour() {
    let states = [
        PluginRowState::Installed,
        PluginRowState::UnsupportedPlatform {
            reason: "no binary for x".to_owned(),
        },
        PluginRowState::Ambiguous {
            code: PluginCode::Ambiguous,
            paths: vec!["/a".to_owned(), "/b".to_owned()],
        },
        PluginRowState::Unavailable {
            reason: "broken".to_owned(),
        },
    ];
    let mut statuses: Vec<String> = states.iter().map(PluginRowState::status).collect();
    let total = statuses.len();
    statuses.sort();
    statuses.dedup();
    assert_eq!(
        statuses.len(),
        total,
        "each state must read differently in plain text"
    );
}

#[test]
fn the_version_chooser_sees_every_installed_version() {
    let snapshot = PluginSnapshotRow {
        versions: vec!["1.0.0".to_owned(), "0.9.0".to_owned()],
        ..row("vendor.git-merger", "1.0.0", PluginRowState::Installed)
    };
    let rows = project(&[snapshot], &none_trusted);
    assert_eq!(
        rows.first().map(|row| row.versions.clone()),
        Some(vec!["1.0.0".to_owned(), "0.9.0".to_owned()])
    );
}

#[test]
fn the_trust_confirmation_states_the_consequence() {
    assert!(
        TRUST_CONFIRMATION.contains("unsandboxed"),
        "the operator must be told the provider is not sandboxed"
    );
    assert!(
        TRUST_CONFIRMATION.contains("as you"),
        "the operator must be told it runs with their own privileges"
    );
}

#[test]
fn the_recovery_notice_states_that_nothing_ran() {
    assert_eq!(RECOVERY_PROCESS_NOTICE, "provider processes started: 0");
}

#[test]
fn projecting_an_empty_inventory_yields_no_rows() {
    assert!(project(&[], &none_trusted).is_empty());
}
