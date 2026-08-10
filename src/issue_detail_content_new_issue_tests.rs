//! Behavioral tests for the New Issue document projection (issue #693).
//!
//! The New Issue form has eight fields but the pane used to render a fixed
//! four-line prompt plus one composer showing only the *focused* field. Typing
//! a subject and then advancing focus made the subject disappear even though it
//! was still stored and still became the created issue's title. These tests
//! pin the document to the form so every field stays visible.

use super::build_new_issue_content;
use crate::state::{NewIssueFormFocus, NewIssueFormState};

/// Every focus value, so a test can prove a field survives any focus change.
const ALL_FOCUS: [NewIssueFormFocus; 8] = [
    NewIssueFormFocus::Template,
    NewIssueFormFocus::Type,
    NewIssueFormFocus::Title,
    NewIssueFormFocus::Body,
    NewIssueFormFocus::Labels,
    NewIssueFormFocus::Milestone,
    NewIssueFormFocus::Project,
    NewIssueFormFocus::Assignees,
];

fn form_with_title(title: &str, focus: NewIssueFormFocus) -> NewIssueFormState {
    NewIssueFormState {
        title_text: title.to_string(),
        title_cursor: title.chars().count(),
        focus,
        ..NewIssueFormState::default()
    }
}

fn lines(form: Option<&NewIssueFormState>) -> Vec<String> {
    build_new_issue_content(form)
        .text
        .lines()
        .map(str::to_string)
        .collect()
}

/// A1 — the reported bug. The subject is typed on Title, focus advances (bare
/// Enter dispatches `NewIssueFocusNext`, issue #480), and the subject must
/// still be on screen for every focus the user can reach.
#[test]
fn typed_title_stays_visible_for_every_focus() {
    for focus in ALL_FOCUS {
        let form = form_with_title("Fix the widget", focus);
        let rendered = build_new_issue_content(Some(&form)).text;
        assert!(
            rendered.contains("Title: Fix the widget"),
            "the typed title must stay visible with focus {focus:?}: {rendered}"
        );
    }
}

/// A2 — the same guarantee for the body: typed body text stays visible after
/// focus moves on to the picker fields.
#[test]
fn typed_body_stays_visible_for_every_focus() {
    for focus in ALL_FOCUS {
        let form = NewIssueFormState {
            body_text: "steps to reproduce".to_string(),
            focus,
            ..NewIssueFormState::default()
        };
        let rendered = build_new_issue_content(Some(&form)).text;
        assert!(
            rendered.contains("Body: steps to reproduce"),
            "the typed body must stay visible with focus {focus:?}: {rendered}"
        );
    }
}

/// A2 — a multi-line body renders one row per line, with continuation rows
/// indented under the label so the block reads as one field.
#[test]
fn multi_line_body_renders_indented_continuation_rows() {
    let form = NewIssueFormState {
        body_text: "first line\nsecond line".to_string(),
        focus: NewIssueFormFocus::Body,
        ..NewIssueFormState::default()
    };
    let rendered = lines(Some(&form));
    assert!(
        rendered.iter().any(|line| line == "> Body: first line"),
        "focused body must render its first line: {rendered:?}"
    );
    assert!(
        rendered.iter().any(|line| line == "        second line"),
        "body continuation must align under the label: {rendered:?}"
    );
}

/// A2 — an empty body still renders its labelled row, so Tab order and the
/// document line count stay stable while the user types.
#[test]
fn empty_body_still_renders_its_row() {
    let form = NewIssueFormState::default();
    let rendered = lines(Some(&form));
    assert!(
        rendered.iter().any(|line| line == "  Body:"),
        "an empty body keeps a labelled row: {rendered:?}"
    );
}

/// A3 — exactly one field row is marked as focused, and it is the focused one.
#[test]
fn exactly_one_row_is_marked_focused() {
    for focus in ALL_FOCUS {
        let form = form_with_title("subject", focus);
        let rendered = lines(Some(&form));
        let marked = rendered
            .iter()
            .filter(|line| line.starts_with("> "))
            .count();
        assert_eq!(
            marked, 1,
            "exactly one row must be marked focused for {focus:?}: {rendered:?}"
        );
    }
}

/// A3 — the focus marker tracks the field the user is editing.
#[test]
fn the_focus_marker_names_the_focused_field() {
    let cases = [
        (NewIssueFormFocus::Template, "> Template:"),
        (NewIssueFormFocus::Type, "> Type:"),
        (NewIssueFormFocus::Title, "> Title:"),
        (NewIssueFormFocus::Body, "> Body:"),
        (NewIssueFormFocus::Labels, "> Labels:"),
        (NewIssueFormFocus::Milestone, "> Milestone:"),
        (NewIssueFormFocus::Project, "> Project:"),
        (NewIssueFormFocus::Assignees, "> Assignees:"),
    ];
    for (focus, expected_prefix) in cases {
        let form = NewIssueFormState {
            focus,
            ..NewIssueFormState::default()
        };
        let rendered = lines(Some(&form));
        assert!(
            rendered
                .iter()
                .any(|line| line.starts_with(expected_prefix)),
            "focus {focus:?} must mark {expected_prefix}: {rendered:?}"
        );
    }
}

/// A4 — unset optional fields render a stable placeholder instead of a ragged
/// empty row, so the form reads as a list of decisions still to make.
#[test]
fn unset_optional_fields_render_a_placeholder() {
    let form = NewIssueFormState::default();
    let rendered = lines(Some(&form));
    for expected in [
        "  Type: (none)",
        "  Labels: (none)",
        "  Milestone: (none)",
        "  Project: (none)",
        "  Assignees: (none)",
    ] {
        assert!(
            rendered.iter().any(|line| line == expected),
            "missing placeholder row {expected}: {rendered:?}"
        );
    }
}

/// A4 — set optional fields render their values rather than the placeholder.
#[test]
fn set_optional_fields_render_their_values() {
    let form = NewIssueFormState {
        type_name: Some("Bug".to_string()),
        labels: vec!["bug".to_string(), "ui".to_string()],
        milestone: Some("v1.2".to_string()),
        project_ids: vec!["PVT_1".to_string()],
        assignees: vec!["acoliver".to_string()],
        ..NewIssueFormState::default()
    };
    let rendered = lines(Some(&form));
    for expected in [
        "  Type: Bug",
        "  Labels: bug, ui",
        "  Milestone: v1.2",
        "  Project: PVT_1",
        "  Assignees: acoliver",
    ] {
        assert!(
            rendered.iter().any(|line| line == expected),
            "missing value row {expected}: {rendered:?}"
        );
    }
}

/// A4 — the template choice is shown by its human label.
#[test]
fn the_template_row_shows_the_selected_template_label() {
    let form = NewIssueFormState {
        template: crate::state::NewIssueTemplate::Bug,
        focus: NewIssueFormFocus::Title,
        ..NewIssueFormState::default()
    };
    let rendered = lines(Some(&form));
    assert!(
        rendered.iter().any(|line| line == "  Template: Bug"),
        "the template row must name the selection: {rendered:?}"
    );
}

/// A5 — rendered rows stay emoji-free ASCII, as the New PR composer does.
#[test]
fn the_new_issue_document_is_emoji_free() {
    let form = form_with_title("subject", NewIssueFormFocus::Title);
    let rendered = build_new_issue_content(Some(&form)).text;
    assert!(
        rendered.is_ascii(),
        "the New Issue document must stay ASCII: {rendered}"
    );
}

/// A8 — the composer anchor stays, because the editable text is still rendered
/// by the embedded wrapping `TextBox` (issue #212); the document must not
/// flatten a cursor of its own.
#[test]
fn the_document_keeps_the_composer_anchor_and_owns_no_cursor() {
    let form = form_with_title("subject", NewIssueFormFocus::Title);
    let content = build_new_issue_content(Some(&form));
    assert!(
        content.text.contains("[Composer input]"),
        "the composer anchor must remain: {}",
        content.text
    );
    assert!(
        content.cursor.is_none(),
        "the caret belongs to the embedded TextBox, not the document"
    );
}

/// A8 — with no form open the document degrades to the bare prompt rather than
/// panicking or rendering stale field rows.
#[test]
fn without_a_form_only_the_prompt_and_anchor_render() {
    let rendered = lines(None);
    assert_eq!(
        rendered.first().map(String::as_str),
        Some("New Issue"),
        "the prompt header must lead: {rendered:?}"
    );
    assert!(
        rendered.iter().any(|line| line == "[Composer input]"),
        "the composer anchor must remain without a form: {rendered:?}"
    );
    assert!(
        !rendered.iter().any(|line| line.contains("Title:")),
        "no field rows may render without a form: {rendered:?}"
    );
}
