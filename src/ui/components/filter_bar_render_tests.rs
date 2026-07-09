//! Render-path tests for the generic `FilterBar` component.
//!
//! These lock the shared structure that `FilterControls` (Issues) and
//! `PrFilterControls` (PRs) delegate to after the refactoring: a bordered
//! column box → rows of labeled `[value]` fields with active-field
//! inverted-color highlighting → an action-hints row. The assertions are
//! structural (visibility toggle, field/value text, active highlight color,
//! action-hint text, row-prefix/continuation-prefix alignment) so they
//! validate the generic renderer independently of the per-domain projections.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-ISS-008
//! @requirement REQ-PR-008

use crate::theme::ThemeColors;
use crate::ui::components::{FilterBarProps, FilterFieldView, filter_bar_element};

use iocraft::prelude::*;

/// Render a `FilterBar` element into an ANSI string at a fixed size.
fn render_ansi(props: FilterBarProps, cols: u16, rows: u16) -> String {
    let mut elem = element! {
        Box(width: u32::from(cols), height: u32::from(rows)) {
            #(vec![filter_bar_element(props)])
        }
    };
    let canvas = elem.render(Some(usize::from(cols)));
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    String::from_utf8_lossy(&buf).into_owned()
}

/// Build a minimal props with two fields (one row) for the basic tests.
fn base_props(row_prefix: &str, continuation_prefix: &str) -> FilterBarProps {
    FilterBarProps {
        fields: vec![
            FilterFieldView {
                label: "state".to_string(),
                value: "open".to_string(),
                active: false,
            },
            FilterFieldView {
                label: "author".to_string(),
                value: "alice".to_string(),
                active: false,
            },
        ],
        visible: true,
        row_prefix: row_prefix.to_string(),
        continuation_prefix: continuation_prefix.to_string(),
        fields_per_row: 4,
        action_hints: vec!["Tab next  ".to_string(), "Esc cancel".to_string()],
        colors: ThemeColors::default(),
    }
}

// ── Visibility toggle ─────────────────────────────────────────────────────

/// When `visible` is false, the component renders a 0×0 box: no border, no
/// field text, no hint text appears in the output.
#[test]
fn filter_bar_renders_empty_box_when_not_visible() {
    let mut props = base_props("Filter: ", "        ");
    props.visible = false;
    let ansi = render_ansi(props, 60, 6);
    assert!(
        !ansi.contains("Filter:"),
        "invisible filter bar must not render field text: {ansi}"
    );
    assert!(
        !ansi.contains("Tab next"),
        "invisible filter bar must not render hints: {ansi}"
    );
    assert!(
        !ansi.contains('╭'),
        "invisible filter bar must not render a border: {ansi}"
    );
}

/// Render a `FilterBar` element into a plain-text canvas (ANSI-stripped) at a
/// fixed size. Used for exact-substring checks on rendered row text.
fn render_plain(props: FilterBarProps, cols: u16, rows: u16) -> String {
    strip_ansi(&render_ansi(props, cols, rows))
}

// ── Field rendering ───────────────────────────────────────────────────────

/// A visible filter bar renders the row prefix, every field label + bracketed
/// value, and every action hint.
#[test]
fn filter_bar_renders_all_fields_and_hints_when_visible() {
    let props = base_props("Filter: ", "        ");
    let ansi = render_ansi(props, 60, 6);
    assert!(ansi.contains("Filter:"), "row prefix must render: {ansi}");
    assert!(ansi.contains("state:"), "first label must render: {ansi}");
    assert!(ansi.contains("[open]"), "first value must render: {ansi}");
    assert!(
        ansi.contains("author:"),
        "second label must render (note: two-space prefix is part of label): {ansi}"
    );
    assert!(ansi.contains("[alice]"), "second value must render: {ansi}");
    assert!(ansi.contains("Tab next"), "first hint must render: {ansi}");
    assert!(ansi.contains("Esc cancel"), "last hint must render: {ansi}");
}

/// The two-space separator between fields in the same row must be part of the
/// second label's Text content (matching the pre-refactor components), not
/// separate spacing. Verified by inspecting the plain-text rendered row:
/// `state:[open]  author:[alice]`.
#[test]
fn filter_bar_two_space_prefix_is_part_of_subsequent_label_text() {
    let props = base_props("Filter: ", "        ");
    let plain = render_plain(props, 60, 6);
    let row = plain
        .lines()
        .find(|line| line.contains("state:[open]"))
        .unwrap_or_else(|| panic!("row with state field must render: {plain}"));
    assert!(
        row.contains("state:[open]  author:[alice]"),
        "two-space prefix must be part of the subsequent label text: {row}"
    );
}

/// When fields exceed one row, the continuation prefix renders on row 2 and
/// row-2 field labels/values appear.
#[test]
fn filter_bar_renders_continuation_prefix_on_second_row() {
    let fields: Vec<FilterFieldView> = (0..8)
        .map(|i| FilterFieldView {
            label: format!("f{i}"),
            value: format!("v{i}"),
            active: false,
        })
        .collect();
    let props = FilterBarProps {
        fields,
        visible: true,
        row_prefix: "Filter: ".to_string(),
        continuation_prefix: "       ".to_string(),
        fields_per_row: 4,
        action_hints: vec![],
        colors: ThemeColors::default(),
    };
    let ansi = render_ansi(props, 80, 6);
    // Row 1 carries the row prefix and fields 0-3; row 2 carries fields 4-7.
    assert!(ansi.contains("f0:"), "row-1 first label: {ansi}");
    assert!(ansi.contains("[v0]"), "row-1 first value: {ansi}");
    assert!(ansi.contains("f4:"), "row-2 first label: {ansi}");
    assert!(ansi.contains("[v4]"), "row-2 first value: {ansi}");
    assert!(ansi.contains("f7:"), "row-2 last label: {ansi}");
    assert!(ansi.contains("[v7]"), "row-2 last value: {ansi}");
}

// ── Active-field highlight color ───────────────────────────────────────────

/// The active field's value renders with the inverted-color background
/// (`rc.bright` = #00ff00 → RGB 0;255;0). The inactive field's value does not
/// carry that background SGR.
#[test]
fn filter_bar_highlights_active_field_with_bright_background() {
    let mut props = base_props("Filter: ", "        ");
    // Make field 0 active, field 1 inactive.
    props.fields[0].active = true;
    let ansi = render_ansi(props, 60, 6);
    // Green-screen bright = #00ff00 → RGB 0;255;0. The active value's Box
    // paints its background with the bright color (SGR 48;2;0;255;0).
    assert!(
        ansi.contains("\u{1b}[48;2;0;255;0m"),
        "active field value must render with bright (0;255;0) background: {ansi}"
    );
    // The active value text itself uses rc.bg (#000000 → 0;0;0).
    assert!(
        ansi.contains("\u{1b}[38;2;0;0;0m"),
        "active field value text must render with bg (0;0;0) foreground: {ansi}"
    );
}

/// When no field is active, no inverted-color background SGR appears.
#[test]
fn filter_bar_renders_no_highlight_when_no_field_active() {
    let props = base_props("Filter: ", "        ");
    let ansi = render_ansi(props, 60, 6);
    assert!(
        !ansi.contains("\u{1b}[48;2;0;255;0m"),
        "no active field → no bright background highlight: {ansi}"
    );
}

// ── Action hints ──────────────────────────────────────────────────────────

/// The action-hints row renders every hint segment in the dim color
/// (`rc.dim` = #6a9955 → RGB 106;153;85) — same as both pre-refactor
/// components which painted all hints `rc.dim`.
#[test]
fn filter_bar_renders_action_hints_in_dim_color() {
    let props = base_props("Filter: ", "        ");
    let ansi = render_ansi(props, 60, 6);
    // Green-screen dim = accent_secondary = #6a9955 → RGB 106;153;85.
    assert!(
        ansi.contains("\u{1b}[38;2;106;153;85m"),
        "action hints must render in dim (106;153;85) color: {ansi}"
    );
    assert!(ansi.contains("Esc cancel"), "last hint text: {ansi}");
}

// ── Alignment parity (Issues vs PR) ────────────────────────────────────────

/// Strip ANSI SGR/cursor escape sequences so spacing assertions see plain text.
fn strip_ansi(ansi: &str) -> String {
    let mut out = String::with_capacity(ansi.len());
    let mut chars = ansi.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            // Consume the CSI sequence: ESC '[' ... letter.
            chars.next();
            for inner in chars.by_ref() {
                if inner.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Count the leading spaces on a rendered row line, skipping the iocraft box
/// border glyph (`│`). Both domains use `padding_left: 1u32`, so the count
/// includes that single padding space plus the continuation-prefix spaces.
fn leading_continuation_spaces(row: &str) -> usize {
    // Drop the leading border glyph (`│`) if present, then count the run of
    // spaces (padding_left + continuation prefix) before the first label.
    let after_border = row.trim_start_matches('│');
    after_border.chars().take_while(|&c| c == ' ').count()
}

/// Issues-style props (8-space continuation prefix) reproduce the exact row-2
/// alignment: 8 leading spaces before the first row-2 label. The rendered
/// output for row 2 starts with the continuation prefix text.
#[test]
fn filter_bar_issues_continuation_prefix_is_eight_spaces() {
    let fields: Vec<FilterFieldView> = (0..8)
        .map(|i| FilterFieldView {
            label: format!("f{i}"),
            value: format!("v{i}"),
            active: false,
        })
        .collect();
    let props = FilterBarProps {
        fields,
        visible: true,
        row_prefix: "Filter: ".to_string(),
        continuation_prefix: "        ".to_string(),
        fields_per_row: 4,
        action_hints: vec![],
        colors: ThemeColors::default(),
    };
    let ansi = render_ansi(props, 80, 6);
    let plain = strip_ansi(&ansi);
    // Row 2 (the line containing f4) must start with 8 continuation-prefix
    // spaces before f4. The count includes the box padding-left (1 space), so
    // the rendered run is 1 (padding) + 8 (continuation) = 9 spaces.
    let row2 = plain
        .lines()
        .find(|line| line.contains("f4"))
        .unwrap_or_else(|| panic!("row containing f4 not found: {plain}"));
    let leading = leading_continuation_spaces(row2);
    assert_eq!(
        leading, 9,
        "Issues row-2 must have 9 rendered spaces (1 padding + 8 continuation) before f4: {row2:?}"
    );
}

/// PR-style props (7-space continuation prefix) reproduce the exact row-2
/// alignment: 7 leading spaces before the first row-2 label.
#[test]
fn filter_bar_pr_continuation_prefix_is_seven_spaces() {
    let fields: Vec<FilterFieldView> = (0..8)
        .map(|i| FilterFieldView {
            label: format!("f{i}"),
            value: format!("v{i}"),
            active: false,
        })
        .collect();
    let props = FilterBarProps {
        fields,
        visible: true,
        row_prefix: "Filter: ".to_string(),
        continuation_prefix: "       ".to_string(),
        fields_per_row: 4,
        action_hints: vec![],
        colors: ThemeColors::default(),
    };
    let ansi = render_ansi(props, 80, 6);
    let plain = strip_ansi(&ansi);
    // Row 2: 1 (padding) + 7 (continuation) = 8 rendered spaces before f4.
    let row2 = plain
        .lines()
        .find(|line| line.contains("f4"))
        .unwrap_or_else(|| panic!("row containing f4 not found: {plain}"));
    let leading = leading_continuation_spaces(row2);
    assert_eq!(
        leading, 8,
        "PR row-2 must have 8 rendered spaces (1 padding + 7 continuation) before f4: {row2:?}"
    );
}
