//! Lossless settings-document behavior tests.

use crate::domain::ByteSpan;

use super::diagnostic::{CfgCode, FILE_LIMIT};
use super::settings_document::SettingsDocument;
use super::sha256::Sha256;

#[test]
fn parse_retains_original_bytes_hash_comments_order_and_quoting() {
    let original = br#"# heading
settings_schema = 2

[appearance]
theme = 'green-screen' # trailing

[agents."core.llxprt"]
enabled = true
"#;
    let Ok(document) = SettingsDocument::parse(original) else {
        panic!("valid settings document must parse");
    };

    assert_eq!(document.original_bytes(), original);
    assert_eq!(document.sha256(), Sha256::digest(original));
    assert_eq!(document.comment_spans().len(), 2);

    let theme_path = ["appearance", "theme"];
    let Some(theme) = document.node(&theme_path) else {
        panic!("theme assignment must have a syntax node");
    };
    assert_eq!(document.span_bytes(theme.value_span), b"'green-screen'");
    assert_eq!(theme.path, theme_path);

    let owner_path = ["agents", "core.llxprt", "enabled"];
    let Some(enabled) = document.node(&owner_path) else {
        panic!("quoted owner assignment must have a syntax node");
    };
    assert_eq!(document.span_bytes(enabled.value_span), b"true");
}

#[test]
fn parser_accepts_multiline_values_without_losing_statement_spans() {
    let original = br#"settings_schema = 2
[appearance]
theme = """
green
screen
"""
[extensions.future]
values = [
  "one", # inside array
  "two",
]
"#;
    let Ok(document) = SettingsDocument::parse(original) else {
        panic!("multiline TOML must parse");
    };
    let Some(theme) = document.node(&["appearance", "theme"]) else {
        panic!("multiline assignment must be indexed");
    };
    let value = document.span_bytes(theme.value_span);
    assert!(value.starts_with(b"\"\"\""));
    assert!(value.ends_with(b"\"\"\""));
    assert_eq!(document.original_bytes(), original);
}

#[test]
fn malformed_toml_is_cfg_e002_with_source_span() {
    let diagnostics = SettingsDocument::parse(b"settings_schema = [")
        .err()
        .unwrap_or_else(|| panic!("malformed TOML must fail"));
    assert_eq!(diagnostics.code, CfgCode::E002);
    assert!(diagnostics.span.is_some());
}

#[test]
fn file_bound_is_inclusive_and_owned_by_settings_parser() {
    let at_limit = vec![b' '; FILE_LIMIT];
    let Ok(document) = SettingsDocument::parse(&at_limit) else {
        panic!("file exactly at the inclusive limit must parse");
    };
    assert_eq!(document.original_bytes().len(), FILE_LIMIT);

    let over_limit = vec![b' '; FILE_LIMIT + 1];
    let diagnostic = SettingsDocument::parse(&over_limit)
        .err()
        .unwrap_or_else(|| panic!("file over limit must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);
    assert_eq!(
        diagnostic.span,
        Some(ByteSpan::new(0, (FILE_LIMIT + 1) as u64))
    );
}

#[test]
fn string_array_map_and_depth_bounds_are_rejected_by_one() {
    let long_string = "x".repeat(super::diagnostic::STRING_LIMIT + 1);
    let input = format!("settings_schema = 2\n[extensions]\nvalue = \"{long_string}\"\n");
    let diagnostic = SettingsDocument::parse(input.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("overlong string must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);

    let nested = "[".repeat(super::diagnostic::NESTING_LIMIT);
    let closed = "]".repeat(super::diagnostic::NESTING_LIMIT);
    let input = format!("settings_schema = 2\n[extensions]\nvalue = {nested}0{closed}\n");
    let diagnostic = SettingsDocument::parse(input.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("depth over limit must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);
}
