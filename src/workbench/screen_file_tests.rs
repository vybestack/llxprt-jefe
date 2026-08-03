//! Closed-syntax parser: what the grammar accepts and what it refuses
//! (issue #385, CW05-02).

use super::screen_file::parse_screen_file;
use super::screen_file::{
    ActivationKind, ActivationModeFile, EmptyPolicyFile, LayoutFile, PortDirectionFile,
    RelationshipFile, SizeFile,
};
use super::screen_file_bounds::ScreenSyntaxReason;
use super::screen_file_fixtures::{HEADER, LAYOUT, PANELS, parsed, rejected, valid_text};

// ── Acceptance ─────────────────────────────────────────────────────────────

#[test]
fn a_complete_valid_file_parses_into_the_closed_syntax() {
    let file = parsed(&valid_text());

    assert_eq!(file.screen_schema, 1);
    assert_eq!(file.id.get_ref(), "local.review");
    assert_eq!(file.title, "Review");
    assert_eq!(file.route, "review");
    assert_eq!(file.initial_focus, "pr-list");
    assert_eq!(file.focus_order, vec!["pr-list", "pr-detail"]);
    assert_eq!(file.panels.len(), 2);
    assert_eq!(file.relationships.len(), 1);
    assert_eq!(file.bindings.len(), 1);
}

#[test]
fn a_port_declaration_carries_its_direction_type_and_retention() {
    let file = parsed(&valid_text());

    let output = &file.panels[0].get_ref().ports[0];
    assert_eq!(output.get_ref().id, "selection");
    assert_eq!(output.get_ref().direction, PortDirectionFile::Output);
    assert_eq!(output.get_ref().type_id, "github.pull-request@1");
    assert!(!output.get_ref().retained);

    let input = &file.panels[1].get_ref().ports[0];
    assert_eq!(input.get_ref().direction, PortDirectionFile::Input);
    assert!(input.get_ref().retained);
}

#[test]
fn a_master_detail_relationship_carries_its_activation_and_empty_policy() {
    let file = parsed(&valid_text());

    match file.relationships[0].get_ref() {
        RelationshipFile::MasterDetail {
            source,
            target,
            activation,
            empty,
        } => {
            assert_eq!(source, "pr-list.selection");
            assert_eq!(target, "pr-detail.subject");
            assert_eq!(*activation, ActivationModeFile::Immediate);
            assert_eq!(*empty, EmptyPolicyFile::Retain);
        }
        other => unreachable!("fixture declares a master-detail relationship, got {other:?}"),
    }
}

#[test]
fn every_relationship_kind_in_the_grammar_parses() {
    let text = format!(
        r#"{HEADER}{PANELS}{LAYOUT}
[[relationships]]
kind = "scope"
source = "pr-list.selection"
target = "pr-detail.subject"

[[relationships]]
kind = "session-target"
source = "pr-list.selection"
target = "pr-detail.subject"
empty = "detach"
"#
    );

    let file = parsed(&text);

    assert!(matches!(
        file.relationships[0].get_ref(),
        RelationshipFile::Scope { .. }
    ));
    assert!(matches!(
        file.relationships[1].get_ref(),
        RelationshipFile::SessionTarget { .. }
    ));
}

#[test]
fn every_activation_field_kind_in_the_grammar_parses() {
    let text = format!(
        r#"{HEADER}{PANELS}{LAYOUT}
[[activation]]
name = "show-drafts"
type = "boolean"

[[activation]]
name = "auto-open"
type = "optional-boolean"

[[activation]]
name = "label"
type = "string"

[[activation]]
name = "page-size"
type = "integer"

[[activation]]
name = "mode"
type = "enum"
values = ["fast", "full"]

[[activation]]
name = "root"
type = "path"

[[activation]]
name = "tags"
type = "string-list"
"#
    );

    let kinds: Vec<ActivationKind> = parsed(&text)
        .activation
        .iter()
        .map(|field| field.get_ref().kind)
        .collect();

    assert_eq!(
        kinds,
        vec![
            ActivationKind::Boolean,
            ActivationKind::OptionalBoolean,
            ActivationKind::String,
            ActivationKind::Integer,
            ActivationKind::Enum,
            ActivationKind::Path,
            ActivationKind::StringList,
        ]
    );
}

#[test]
fn a_fixed_size_and_a_weighted_size_are_distinguishable() {
    let layout = r#"
[layout]
type = "split"
axis = "vertical"

[[layout.children]]
min = 1
collapsible = false
size = { fixed = 3 }
node = { type = "leaf", panel = "pr-list" }

[[layout.children]]
min = 1
collapsible = false
size = { weight = 7 }
node = { type = "leaf", panel = "pr-detail" }
"#;
    let text = format!("{HEADER}{PANELS}{layout}");

    let file = parsed(&text);

    match &file.layout {
        LayoutFile::Split { children, .. } => {
            assert_eq!(children[0].size, SizeFile::Fixed(3));
            assert_eq!(children[1].size, SizeFile::Weight(7));
        }
        other @ LayoutFile::Leaf { .. } => {
            unreachable!("fixture declares a split, got {other:?}")
        }
    }
}

// ── Closure ────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_top_level_field_is_rejected() {
    let text = format!("{HEADER}extra = true\n{PANELS}{LAYOUT}");

    assert!(matches!(
        rejected(&text),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

#[test]
fn an_unknown_nested_field_is_rejected() {
    let panels = PANELS.replace(
        "type = \"pull-request-list\"",
        "type = \"pull-request-list\"\nunknown = 1",
    );
    let text = format!("{HEADER}{panels}{LAYOUT}");

    assert!(matches!(
        rejected(&text),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

#[test]
fn a_duplicate_key_is_rejected() {
    let text = format!("{HEADER}title = \"Second\"\n{PANELS}{LAYOUT}");

    assert!(matches!(
        rejected(&text),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

#[test]
fn a_missing_required_field_is_rejected_rather_than_defaulted() {
    let panels = PANELS.replace("focusable = true\nrequired = true", "required = true");
    let text = format!("{HEADER}{panels}{LAYOUT}");

    assert!(matches!(
        rejected(&text),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

#[test]
fn a_value_outside_a_closed_enumeration_is_rejected() {
    for (from, to) in [
        ("direction = \"output\"", "direction = \"in\""),
        ("axis = \"horizontal\"", "axis = \"diagonal\""),
        ("activation = \"immediate\"", "activation = \"eventually\""),
        ("empty = \"retain\"", "empty = \"show-something\""),
        ("kind = \"master-detail\"", "kind = \"sibling\""),
    ] {
        let text = valid_text().replace(from, to);
        assert!(
            matches!(rejected(&text), ScreenSyntaxReason::Malformed { .. }),
            "{to} must be rejected"
        );
    }
}

#[test]
fn a_secret_activation_field_kind_has_no_spelling() {
    let text =
        format!("{HEADER}{PANELS}{LAYOUT}\n[[activation]]\nname = \"token\"\ntype = \"secret\"\n");

    assert!(matches!(
        rejected(&text),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

#[test]
fn an_unsupported_schema_version_is_rejected_by_number() {
    let text = valid_text().replace("screen_schema = 1", "screen_schema = 2");

    assert_eq!(
        rejected(&text),
        ScreenSyntaxReason::UnsupportedSchema { found: 2 }
    );
}

#[test]
fn text_that_is_not_toml_is_rejected() {
    assert!(matches!(
        rejected("this is not toml {{{"),
        ScreenSyntaxReason::Malformed { .. }
    ));
}

// ── Redaction and reference shape (issue #385 review remediation) ──────────

#[test]
fn a_type_mismatch_report_names_the_types_without_repeating_the_value() {
    let text = valid_text().replacen("screen_schema = 1", "screen_schema = \"tok-abc123\"", 1);

    let reason = rejected(&text);

    let rendered = reason.to_string();
    assert!(
        !rendered.contains("tok-abc123"),
        "a diagnostic must not repeat what the file said, got {rendered:?}"
    );
    assert!(
        rendered.contains("invalid type"),
        "a diagnostic must still say what was wrong, got {rendered:?}"
    );
}

#[test]
fn an_unsupported_schema_is_reported_before_any_other_field_is_interpreted() {
    // The file is also missing a required field; an author running the wrong
    // version needs to hear about the version, not about a field their schema
    // spells differently.
    let text = "screen_schema = 7\n";

    assert_eq!(
        rejected(text),
        ScreenSyntaxReason::UnsupportedSchema { found: 7 }
    );
}

#[test]
fn a_panel_or_port_identifier_may_not_contain_the_reference_separator() {
    let panel = valid_text().replacen("id = \"pr-list\"", "id = \"pr.list\"", 1);
    let port = valid_text().replacen("id = \"selection\"", "id = \"sel.ection\"", 1);

    assert_eq!(
        rejected(&panel),
        ScreenSyntaxReason::SeparatorInComponent { field: "panels.id" }
    );
    assert_eq!(
        rejected(&port),
        ScreenSyntaxReason::SeparatorInComponent { field: "ports.id" }
    );
}

#[test]
fn a_relationship_endpoint_must_be_spelled_panel_dot_port() {
    let text = valid_text().replacen("source = \"pr-list.selection\"", "source = \"pr-list\"", 1);

    assert_eq!(rejected(&text), ScreenSyntaxReason::MalformedPortReference);
}

#[test]
fn an_enum_field_declaring_an_empty_value_list_is_rejected() {
    let text = format!(
        "{HEADER}{PANELS}{LAYOUT}\n[[activation]]\nname = \"mode\"\ntype = \"enum\"\nvalues = []\n"
    );

    assert_eq!(
        rejected(&text),
        ScreenSyntaxReason::EnumValuesMismatch { is_enum: true }
    );
}

// ── Epic identifier grammar (issue #385, epic global grammar) ──────────────

#[test]
fn a_declared_identifier_must_match_the_workbench_grammar() {
    // The epic's identifier grammar is lowercase alphanumerics in
    // hyphen-separated groups. Definitions are held to it rather than to the
    // wider grammar the compiled newtypes happen to accept.
    for (from, to, field) in [
        ("id = \"pr-list\"", "id = \"pr_list\"", "panels.id"),
        ("id = \"selection\"", "id = \"my_selection\"", "ports.id"),
        ("route = \"review\"", "route = \"my_review\"", "route"),
    ] {
        let text = valid_text().replacen(from, to, 1);
        assert_eq!(
            rejected(&text),
            ScreenSyntaxReason::MalformedIdentifier { field },
            "{to} must be rejected"
        );
    }
}

#[test]
fn a_declared_identifier_may_not_carry_an_empty_or_trailing_group() {
    for spelling in ["pr--list", "pr-list-", "-pr-list"] {
        let text = valid_text().replacen("id = \"pr-list\"", &format!("id = \"{spelling}\""), 1);
        assert_eq!(
            rejected(&text),
            ScreenSyntaxReason::MalformedIdentifier { field: "panels.id" },
            "{spelling} must be rejected"
        );
    }
}

#[test]
fn a_route_may_carry_dotted_labels_but_a_panel_may_not() {
    let dotted_route = valid_text().replacen("route = \"review\"", "route = \"local.review\"", 1);

    assert!(
        parse_screen_file(&dotted_route).is_ok(),
        "a route is a namespaced identifier, so dots are part of its grammar"
    );
}
