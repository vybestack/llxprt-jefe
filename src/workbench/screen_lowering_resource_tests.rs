//! Definition-owned resource schema lowering tests for issue #705.

use std::path::Path;

use super::lowering_error::LoweringError;
use super::screen_file::parse_screen_file;
use super::screen_file_fixtures::{HEADER, LAYOUT, PANELS};
use super::screen_lowering::lower_package_screen;

const RESOURCE: &str = r#"
[[resources]]
type_id = "local.review.note"
schema_version = 1
semantic_key = "semantic-key"

[[resources.fields]]
id = "semantic-key"
label = "Semantic key"
type = "string"
required = true
"#;

#[test]
fn lowering_derives_resource_ownership_from_the_definition_identity() {
    let text = format!("{HEADER}{RESOURCE}{PANELS}{LAYOUT}")
        .replace("screen_schema = 1", "screen_schema = 2");
    let file = parse_screen_file(&text)
        .unwrap_or_else(|error| unreachable!("resource fixture must parse: {error}"));

    let lowered = lower_package_screen(
        &file,
        "local.review",
        &["pull-request-list", "pull-request-detail"],
        Path::new("review.screen.toml"),
    )
    .unwrap_or_else(|error| unreachable!("resource fixture must lower: {error}"));

    assert_eq!(lowered.resources.len(), 1);
    let schema = &lowered.resources[0];
    assert_eq!(schema.owner_id().as_str(), "local.review");
    assert_eq!(schema.type_id().as_str(), "local.review.note");
    assert_eq!(schema.schema_version(), 1);
}

#[test]
fn legacy_schema_one_ports_use_the_closed_historical_resource_owner_mapping() {
    let text =
        format!("{HEADER}{PANELS}{LAYOUT}").replace("owner = \"github.pull-requests\"\n", "");
    let file = parse_screen_file(&text)
        .unwrap_or_else(|error| unreachable!("legacy fixture must parse: {error}"));

    let lowered = lower_package_screen(
        &file,
        "local.review",
        &["pull-request-list", "pull-request-detail"],
        Path::new("review.screen.toml"),
    )
    .unwrap_or_else(|error| unreachable!("legacy fixture must lower: {error}"));

    for panel in &lowered.descriptor.panels {
        for port in &panel.ports {
            assert_eq!(port.owner_id.as_str(), "github.pull-requests");
        }
    }
}

#[test]
fn schema_one_explicit_owners_must_match_the_closed_historical_mapping() {
    let mismatched =
        format!("{HEADER}{PANELS}{LAYOUT}").replace("github.pull-requests", "hostile.owner");
    let file = parse_screen_file(&mismatched)
        .unwrap_or_else(|error| unreachable!("closed legacy fixture must parse: {error}"));
    assert!(matches!(
        lower_package_screen(
            &file,
            "local.review",
            &["pull-request-list", "pull-request-detail"],
            Path::new("review.screen.toml"),
        ),
        Err(LoweringError::LegacyResourceOwner { type_id })
            if type_id == "github.pull-request@1"
    ));

    let unknown = format!("{HEADER}{PANELS}{LAYOUT}")
        .replace("owner = \"github.pull-requests\"\n", "")
        .replace("github.pull-request@1", "vendor.unknown@1");
    let file = parse_screen_file(&unknown)
        .unwrap_or_else(|error| unreachable!("owner-less legacy fixture must parse: {error}"));
    assert!(matches!(
        lower_package_screen(
            &file,
            "local.review",
            &["pull-request-list", "pull-request-detail"],
            Path::new("review.screen.toml"),
        ),
        Err(LoweringError::LegacyResourceOwner { type_id })
            if type_id == "vendor.unknown@1"
    ));
}
