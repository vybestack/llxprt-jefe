//! Boundary table for [`PluginId`] (issue #389 CW-09, acceptance row D2).

use super::*;
use crate::domain::plugin::limits::PLUGIN_ID_BYTE_LIMIT;

fn reason(value: &str) -> PluginIdErrorReason {
    PluginId::parse(value).err().map_or_else(
        || panic!("{value:?} must be rejected"),
        |error| error.reason,
    )
}

#[test]
fn accepts_the_minimum_two_label_identifier() {
    let id = PluginId::parse("a.b").unwrap_or_else(|error| panic!("a.b must parse: {error}"));
    assert_eq!(id.as_str(), "a.b");
}

#[test]
fn preserves_the_exact_identifier_bytes() {
    for value in ["vendor.git-merger", "a.b.c.d", "x9.y0-z1"] {
        let id =
            PluginId::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"));
        assert_eq!(id.as_str(), value, "{value} must round-trip exactly");
    }
}

#[test]
fn accepts_exactly_the_byte_limit_and_rejects_one_more() {
    // `a.` plus filler keeps the two-label rule while hitting the byte bound.
    let at_limit = format!("a.{}", "b".repeat(PLUGIN_ID_BYTE_LIMIT - 2));
    assert_eq!(at_limit.len(), PLUGIN_ID_BYTE_LIMIT);
    assert!(
        PluginId::parse(&at_limit).is_ok(),
        "an identifier of exactly {PLUGIN_ID_BYTE_LIMIT} bytes must be accepted"
    );

    let over_limit = format!("a.{}", "b".repeat(PLUGIN_ID_BYTE_LIMIT - 1));
    assert_eq!(over_limit.len(), PLUGIN_ID_BYTE_LIMIT + 1);
    assert_eq!(reason(&over_limit), PluginIdErrorReason::Length);
}

#[test]
fn rejects_a_single_label_identifier() {
    for value in ["a", "vendor", "git-merger", "x9-y0"] {
        assert_eq!(
            reason(value),
            PluginIdErrorReason::TooFewLabels,
            "{value} has one label and must be rejected"
        );
    }
}

#[test]
fn rejects_every_reserved_prefix() {
    for value in ["core.dashboard", "github.issues", "local.scratch"] {
        assert_eq!(
            reason(value),
            PluginIdErrorReason::ReservedPrefix,
            "{value} must be rejected as reserved"
        );
    }
}

#[test]
fn reserved_prefixes_match_only_a_whole_first_label() {
    for value in ["corex.thing", "githubby.thing", "locale.thing"] {
        assert!(
            PluginId::parse(value).is_ok(),
            "{value} only shares a prefix with a reserved label and must parse"
        );
    }
}

#[test]
fn rejects_identifiers_outside_the_grammar() {
    for value in [
        "",
        "A.b",
        "a.B",
        "1a.b",
        ".a.b",
        "a.b.",
        "a..b",
        "a.-b",
        "a-.b",
        "a b",
        "a_b.c",
        "a.b\u{e9}",
        "a/b.c",
    ] {
        assert_eq!(
            reason(value),
            PluginIdErrorReason::Grammar,
            "{value:?} must be rejected by the grammar"
        );
    }
}

#[test]
fn error_display_names_the_rejected_value_and_reason() {
    let error = PluginId::parse("core.dashboard")
        .err()
        .unwrap_or_else(|| panic!("reserved prefix must be rejected"));
    let text = error.to_string();
    assert!(
        text.contains("core.dashboard"),
        "display must name the value: {text}"
    );
    assert!(
        text.contains("reserved"),
        "display must name the reason: {text}"
    );
}

#[test]
fn a_plugin_id_is_also_a_configuration_owner_id() {
    let id =
        PluginId::parse("vendor.git-merger").unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(id.owner_id().as_str(), "vendor.git-merger");
}
