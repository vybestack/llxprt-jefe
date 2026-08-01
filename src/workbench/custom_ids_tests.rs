//! Grammar and bound tests for the identity vocabulary externally authored
//! screens introduce (issue #385).

use super::ids::{
    CUSTOM_MEMBER_BYTE_LIMIT, CustomScreenId, ID_BYTE_LIMIT, IdError, MAX_ACTIVATION_FIELDS,
    MAX_BINDINGS_PER_SCREEN, MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN, ScreenId,
    ScreenIdentity, VersionedTypeId,
};
use super::intern::intern;

fn interned(value: String) -> &'static str {
    intern(&value).unwrap_or_else(|error| unreachable!("test identifier must intern: {error}"))
}

// ── CustomScreenId ─────────────────────────────────────────────────────────

#[test]
fn a_custom_screen_identity_must_sit_in_the_local_namespace() {
    assert_eq!(
        CustomScreenId::parse("core.review").map(CustomScreenId::as_str),
        Err(IdError::NotCustomNamespace)
    );
    assert_eq!(
        CustomScreenId::parse("github.review").map(CustomScreenId::as_str),
        Err(IdError::NotCustomNamespace)
    );
    assert_eq!(
        CustomScreenId::parse("review").map(CustomScreenId::as_str),
        Err(IdError::NotCustomNamespace)
    );
}

#[test]
fn a_custom_screen_identity_exposes_its_member_as_the_file_name_stem() {
    let id = CustomScreenId::parse("local.review-board")
        .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}"));

    assert_eq!(id.as_str(), "local.review-board");
    assert_eq!(id.member(), "review-board");
}

#[test]
fn a_custom_member_at_the_declared_limit_is_accepted_and_one_over_is_rejected() {
    let at_limit = interned(format!(
        "local.a{}",
        "b".repeat(CUSTOM_MEMBER_BYTE_LIMIT - 1)
    ));
    let over_limit = interned(format!("local.a{}", "b".repeat(CUSTOM_MEMBER_BYTE_LIMIT)));

    assert!(CustomScreenId::parse(at_limit).is_ok());
    assert_eq!(
        CustomScreenId::parse(over_limit).map(CustomScreenId::as_str),
        Err(IdError::InvalidCustomMember)
    );
}

#[test]
fn a_custom_member_must_start_with_a_lowercase_letter() {
    for rejected in ["local.9review", "local.-review", "local."] {
        assert_eq!(
            CustomScreenId::parse(rejected).map(CustomScreenId::as_str),
            Err(IdError::InvalidCustomMember),
            "{rejected} must not be a custom screen identity"
        );
    }
}

#[test]
fn a_custom_member_admits_only_lowercase_digits_and_hyphen() {
    for rejected in [
        "local.Review",
        "local.re_view",
        "local.re.view",
        "local.re view",
        "local.rev\u{00e9}w",
    ] {
        assert_eq!(
            CustomScreenId::parse(rejected).map(CustomScreenId::as_str),
            Err(IdError::InvalidCustomMember),
            "{rejected} must not be a custom screen identity"
        );
    }
}

// ── ScreenIdentity ─────────────────────────────────────────────────────────

#[test]
fn a_compiled_identity_reports_its_routable_screen_and_a_custom_one_does_not() {
    let compiled = ScreenIdentity::Compiled(ScreenId::Issues);
    let custom = ScreenIdentity::Custom(
        CustomScreenId::parse("local.review")
            .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}")),
    );

    assert_eq!(compiled.compiled(), Some(ScreenId::Issues));
    assert_eq!(custom.compiled(), None);
    assert_eq!(compiled.as_str(), "github.issues");
    assert_eq!(custom.as_str(), "local.review");
}

#[test]
fn every_identity_checks_against_its_own_grammar() {
    assert_eq!(
        ScreenIdentity::Compiled(ScreenId::Dashboard).check(),
        Ok(())
    );
    assert_eq!(
        ScreenIdentity::Custom(
            CustomScreenId::parse("local.review")
                .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}"))
        )
        .check(),
        Ok(())
    );
}

// ── VersionedTypeId ────────────────────────────────────────────────────────

#[test]
fn a_versioned_type_splits_into_its_name_and_version() {
    let id = VersionedTypeId::parse("github.issue@2")
        .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}"));

    assert_eq!(id.as_str(), "github.issue@2");
    assert_eq!(id.name(), "github.issue");
    assert_eq!(id.version(), "2");
}

#[test]
fn a_versioned_type_without_a_version_is_rejected() {
    assert_eq!(
        VersionedTypeId::parse("github.issue").map(VersionedTypeId::as_str),
        Err(IdError::MissingTypeVersion)
    );
}

#[test]
fn a_type_version_must_be_a_positive_integer_with_one_spelling() {
    for rejected in [
        "github.issue@",
        "github.issue@0",
        "github.issue@01",
        "github.issue@1.0",
        "github.issue@v1",
        "github.issue@-1",
    ] {
        assert_eq!(
            VersionedTypeId::parse(rejected).map(VersionedTypeId::as_str),
            Err(IdError::InvalidTypeVersion),
            "{rejected} must not be a versioned type"
        );
    }
}

#[test]
fn a_versioned_type_name_follows_the_plain_identifier_grammar() {
    assert_eq!(
        VersionedTypeId::parse("GitHub.issue@1").map(VersionedTypeId::as_str),
        Err(IdError::InvalidByte)
    );
    assert_eq!(
        VersionedTypeId::parse(".issue@1").map(VersionedTypeId::as_str),
        Err(IdError::LeadingSeparator)
    );
}

#[test]
fn a_versioned_type_at_the_identifier_limit_is_accepted_and_one_over_is_rejected() {
    let name_len = ID_BYTE_LIMIT - "@1".len();
    let at_limit = interned(format!("{}@1", "a".repeat(name_len)));
    let over_limit = interned(format!("{}@1", "a".repeat(name_len + 1)));

    assert!(VersionedTypeId::parse(at_limit).is_ok());
    assert_eq!(
        VersionedTypeId::parse(over_limit).map(VersionedTypeId::as_str),
        Err(IdError::TooLong)
    );
}

#[test]
fn two_versions_of_one_type_are_distinct_identities() {
    let first = VersionedTypeId::parse("github.issue@1")
        .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}"));
    let second = VersionedTypeId::parse("github.issue@2")
        .unwrap_or_else(|error| unreachable!("fixture identifier must parse: {error}"));

    assert_ne!(first, second);
    assert_eq!(first.name(), second.name());
}

// ── Declared bounds ────────────────────────────────────────────────────────

#[test]
fn the_declared_custom_screen_bounds_match_the_closed_syntax() {
    assert_eq!(MAX_PORTS_PER_PANEL, 32);
    assert_eq!(MAX_RELATIONSHIPS_PER_SCREEN, 64);
    assert_eq!(MAX_ACTIVATION_FIELDS, 32);
    assert_eq!(MAX_BINDINGS_PER_SCREEN, 256);
    assert_eq!(CUSTOM_MEMBER_BYTE_LIMIT, 63);
}
