//! Grammar and bound tests for the workbench identifier vocabulary (issue #384,
//! CW04-01).

use super::ids::{
    ID_BYTE_LIMIT, IdError, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN, MAX_SCREENS,
    MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN, PanelId, PanelTypeId, RouteId, ScreenId,
    ScreenInstanceId,
};

#[test]
fn screen_id_accepts_each_reserved_namespace() {
    for value in ["core.dashboard", "github.issues", "local.scratch"] {
        let parsed = ScreenId::parse(value);
        assert!(parsed.is_ok(), "expected {value} to parse");
        assert_eq!(
            parsed.map(|id| id.as_str().to_owned()),
            Ok(value.to_owned())
        );
    }
}

#[test]
fn screen_id_rejects_unreserved_namespace() {
    assert_eq!(
        ScreenId::parse("plugin.dashboard"),
        Err(IdError::UnknownNamespace)
    );
}

#[test]
fn screen_id_rejects_missing_namespace() {
    assert_eq!(ScreenId::parse("dashboard"), Err(IdError::UnknownNamespace));
}

#[test]
fn screen_id_rejects_empty_segment_after_namespace() {
    assert_eq!(ScreenId::parse("core."), Err(IdError::TrailingSeparator));
}

#[test]
fn screen_id_rejects_uppercase() {
    assert_eq!(ScreenId::parse("core.Dashboard"), Err(IdError::InvalidByte));
}

#[test]
fn screen_id_rejects_empty() {
    assert_eq!(ScreenId::parse(""), Err(IdError::Empty));
}

/// 129 bytes: `core.` plus 124 letters is exactly the limit, so one more
/// letter is the first rejected length.
const OVER_LIMIT_SCREEN_ID: &str = "core.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// 128 bytes: `core.` plus 123 letters.
const AT_LIMIT_SCREEN_ID: &str = "core.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn screen_id_rejects_value_over_the_byte_limit() {
    assert_eq!(OVER_LIMIT_SCREEN_ID.len(), ID_BYTE_LIMIT + 1);
    assert_eq!(ScreenId::parse(OVER_LIMIT_SCREEN_ID), Err(IdError::TooLong));
}

#[test]
fn screen_id_accepts_value_exactly_at_the_byte_limit() {
    assert_eq!(AT_LIMIT_SCREEN_ID.len(), ID_BYTE_LIMIT);
    assert!(ScreenId::parse(AT_LIMIT_SCREEN_ID).is_ok());
}

#[test]
fn panel_id_accepts_plain_hyphenated_names() {
    for value in ["repositories", "issue-list", "pr_actions", "agents"] {
        assert!(
            PanelId::parse(value).is_ok(),
            "expected {value} to parse as a panel id"
        );
    }
}

#[test]
fn panel_id_rejects_uppercase_and_spaces() {
    assert_eq!(PanelId::parse("Issue List"), Err(IdError::InvalidByte));
}

#[test]
fn panel_id_rejects_leading_separator() {
    assert_eq!(PanelId::parse("-list"), Err(IdError::LeadingSeparator));
}

#[test]
fn panel_id_rejects_trailing_separator() {
    assert_eq!(PanelId::parse("list-"), Err(IdError::TrailingSeparator));
}

#[test]
fn panel_id_rejects_doubled_separator() {
    assert_eq!(
        PanelId::parse("issue..list"),
        Err(IdError::DoubledSeparator)
    );
}

#[test]
fn route_and_panel_type_ids_share_the_plain_grammar() {
    assert!(RouteId::parse("dashboard").is_ok());
    assert!(PanelTypeId::parse("repository-list").is_ok());
    assert_eq!(RouteId::parse(""), Err(IdError::Empty));
    assert_eq!(PanelTypeId::parse("PTY"), Err(IdError::InvalidByte));
}

#[test]
fn screen_instance_ids_are_distinct_and_monotonic() {
    let first = ScreenInstanceId::next();
    let second = ScreenInstanceId::next();
    assert_ne!(first, second);
    assert!(second > first);
}

#[test]
fn declared_limits_match_the_specified_bounds() {
    assert_eq!(MAX_SCREENS, 64);
    assert_eq!(MAX_PANELS_PER_SCREEN, 16);
    assert_eq!(MIN_SPLIT_CHILDREN, 2);
    assert_eq!(MAX_SPLIT_CHILDREN, 8);
    assert_eq!(MAX_LAYOUT_DEPTH, 8);
    assert_eq!(ID_BYTE_LIMIT, 128);
}

#[test]
fn id_errors_describe_the_violated_rule() {
    for error in [
        IdError::Empty,
        IdError::TooLong,
        IdError::InvalidByte,
        IdError::LeadingSeparator,
        IdError::TrailingSeparator,
        IdError::DoubledSeparator,
        IdError::UnknownNamespace,
    ] {
        assert!(
            !error.description().is_empty(),
            "every id error needs a description"
        );
    }
}
