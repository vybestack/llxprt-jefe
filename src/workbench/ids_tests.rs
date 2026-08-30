//! Grammar and bound tests for the workbench identifier vocabulary (issue #384,
//! CW04-01).

use super::ids::{
    ID_BYTE_LIMIT, IdError, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN, MAX_SCREENS,
    MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN, PanelId, PanelInstanceAllocator, PanelInstanceId,
    PanelTypeId, RouteId, SCREEN_NAMESPACES, ScreenId, ScreenInstanceAllocator, ScreenInstanceId,
};

#[test]
fn every_screen_identity_sits_in_a_reserved_namespace() {
    for id in ScreenId::ALL {
        assert_eq!(id.check(), Ok(()), "screen {id} must satisfy the grammar");
        assert!(
            SCREEN_NAMESPACES
                .iter()
                .any(|namespace| id.as_str().starts_with(namespace)),
            "screen {id} must be namespaced"
        );
    }
}

#[test]
fn screen_identities_are_distinct() {
    for (index, id) in ScreenId::ALL.into_iter().enumerate() {
        assert!(
            !ScreenId::ALL[..index]
                .iter()
                .any(|prior| prior.as_str() == id.as_str()),
            "screen identity {id} is declared twice"
        );
    }
}

#[test]
fn a_screen_resolves_from_its_stable_identity_string() {
    for id in ScreenId::ALL {
        assert_eq!(ScreenId::from_stable(id.as_str()), Some(id));
    }
}

#[test]
fn an_unknown_stable_identity_resolves_to_nothing() {
    for value in ["", "dashboard", "core.nonesuch", "plugin.dashboard", "0"] {
        assert_eq!(
            ScreenId::from_stable(value),
            None,
            "{value} must not resolve to a screen"
        );
    }
}

#[test]
fn screen_resolution_does_not_depend_on_declaration_position() {
    // Resolving by string means reordering the enum cannot change which screen
    // a restored session opens on.
    assert_eq!(
        ScreenId::from_stable("core.repositories"),
        Some(ScreenId::Repositories)
    );
    assert_eq!(
        ScreenId::from_stable("github.issues"),
        Some(ScreenId::Issues)
    );
}

#[test]
fn dashboard_is_not_a_compiled_residual_screen() {
    assert_eq!(ScreenId::from_stable("core.dashboard"), None);
    assert_eq!(ScreenId::from_stable("core.terminals"), None);
    assert_eq!(ScreenId::ALL.len(), 6);
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
fn panel_id_rejects_empty() {
    assert_eq!(PanelId::parse(""), Err(IdError::Empty));
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

/// 129 bytes: one over the limit.
const OVER_LIMIT_PANEL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// 128 bytes: exactly the limit.
const AT_LIMIT_PANEL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn panel_id_rejects_value_over_the_byte_limit() {
    assert_eq!(OVER_LIMIT_PANEL_ID.len(), ID_BYTE_LIMIT + 1);
    assert_eq!(PanelId::parse(OVER_LIMIT_PANEL_ID), Err(IdError::TooLong));
}

#[test]
fn panel_id_accepts_value_exactly_at_the_byte_limit() {
    assert_eq!(AT_LIMIT_PANEL_ID.len(), ID_BYTE_LIMIT);
    assert!(PanelId::parse(AT_LIMIT_PANEL_ID).is_ok());
}

#[test]
fn route_and_panel_type_ids_share_the_plain_grammar() {
    assert!(RouteId::parse("dashboard").is_ok());
    assert!(PanelTypeId::parse("repository-list").is_ok());
    assert_eq!(RouteId::parse(""), Err(IdError::Empty));
    assert_eq!(PanelTypeId::parse("PTY"), Err(IdError::InvalidByte));
}

#[test]
fn a_declared_identifier_is_checked_even_though_it_skipped_parsing() {
    // `from_static` cannot validate in a const context, so `check` is what
    // catches a malformed compiled-in literal.
    assert_eq!(
        PanelId::from_static("Bad Panel").check(),
        Err(IdError::InvalidByte)
    );
    assert_eq!(PanelId::from_static("issue-list").check(), Ok(()));
}

#[test]
fn screen_instance_ids_are_distinct_and_monotonic() {
    let first = ScreenInstanceId::next();
    let second = ScreenInstanceId::next();
    assert_ne!(first, second);
    assert!(second > first);
}

#[test]
fn screen_instance_allocator_refuses_exhaustion_without_wrapping_or_reusing_zero() {
    let allocator = ScreenInstanceAllocator::starting_at(u64::MAX - 1);

    assert_eq!(
        allocator.next().map(ScreenInstanceId::get),
        Ok(u64::MAX - 1)
    );
    assert!(allocator.next().is_err());
    assert!(allocator.next().is_err());

    let zero = ScreenInstanceAllocator::starting_at(0);
    assert!(zero.next().is_err());
}

#[test]
fn panel_instance_allocator_refuses_exhaustion_without_wrapping_or_reusing_zero() {
    let allocator = PanelInstanceAllocator::starting_at(u64::MAX - 1);

    assert_eq!(
        allocator.next().map(PanelInstanceId::as_u64),
        Ok(u64::MAX - 1)
    );
    assert!(allocator.next().is_err());
    assert!(allocator.next().is_err());

    let zero = PanelInstanceAllocator::starting_at(0);
    assert!(zero.next().is_err());
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
