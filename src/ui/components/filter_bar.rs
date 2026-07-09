//! Generic bordered filter bar with labeled `[value]` fields and action hints.
//!
//! Both the Issue (`FilterControls`) and PR (`PrFilterControls`) filter
//! controls render the identical structure: a bordered column box → rows of
//! labeled `[value]` fields (N per row) with active-field inverted-color
//! highlighting → an action-hints row. This module owns that iocraft structure
//! once; the per-domain projection modules (`filter_controls`, `pr_filter_controls`)
//! build a [`FilterBarProps`] carrying the field views, row prefixes,
//! fields-per-row count, action hints, and theme colors, then delegate
//! rendering through [`filter_bar_element`].
//!
//! The active-field color logic (inverted colors on the active field) lives
//! here because it needs iocraft `Color`/`ResolvedColors`; the per-domain
//! projections stay iocraft-free (pure-views pattern).
//!
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-ISS-008
//! @requirement REQ-PR-008

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// A single filter field for display, projected by the domain layer.
///
/// Carries the label, the display value (without brackets), and the
/// active-highlight flag. Owned `String` labels let the same type serve both
/// the Issue (inline string literals) and PR (`&'static str` label table)
/// domains. The component resolves the active/inactive colors against
/// [`ResolvedColors`], so this struct stays free of iocraft types (pure-views
/// pattern).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterFieldView {
    /// Field label (e.g. "state", "author").
    pub label: String,
    /// Display value WITHOUT brackets (e.g. "open", "any", "alice").
    pub value: String,
    /// Whether this field is the active (highlighted) field.
    pub active: bool,
}

/// Props for the generic [`FilterBar`] component.
///
/// The projection owns the field computation, the row-prefix text, the
/// fields-per-row count, and the action-hint list; the component is a pure
/// renderer that applies the shared color logic and box structure.
///
/// `row_prefix`, `continuation_prefix`, and `action_hints` use `&'static str`
/// because both domains (Issues and PRs) supply compile-time constant strings
/// — this avoids per-render heap allocations.
#[derive(Default, Props)]
pub struct FilterBarProps {
    /// Projected field views, in render order (row-major).
    pub fields: Vec<FilterFieldView>,
    /// Whether the controls are visible (false → 0×0 box).
    pub visible: bool,
    /// Text before the first field on row 1 (e.g. `"Filter: "`).
    pub row_prefix: &'static str,
    /// Alignment padding for row 2+ (matches row_prefix width per domain).
    pub continuation_prefix: &'static str,
    /// Number of fields per row (both domains use 4).
    pub fields_per_row: usize,
    /// Action hint segments rendered in the final row.
    pub action_hints: Vec<&'static str>,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Build the two-element group (label `Text` + value `Box`) for a single
/// field with active/inactive colors.
///
/// The label text carries its own leading padding (the first field in a row
/// has no leading spaces; subsequent fields have `"  "` prepended by the
/// caller building the row). The value renders inside a `Box` whose background
/// flips to `rc.bright` when the field is active. Returns a two-element vec so
/// the row `Box` can flatten label + value as direct children (matching the
/// pre-refactor element tree for byte-identical output).
fn field_group(field: &FilterFieldView, rc: ResolvedColors) -> Vec<AnyElement<'static>> {
    let val_color = if field.active { rc.bg } else { rc.fg };
    let val_bg = if field.active { rc.bright } else { rc.bg };
    let label_color = if field.active { rc.bright } else { rc.dim };
    let label = element! {
        Text(content: format!("{}:", field.label), color: label_color)
    }
    .into_any();
    let value = element! {
        Box(background_color: val_bg) {
            Text(content: format!("[{}]", field.value), color: val_color)
        }
    }
    .into_any();
    vec![label, value]
}

/// Build the children for one field row: the row-prefix `Text`, then the
/// [`field_group`] elements for each field. The first field in the row
/// renders its label as `"{label}:"`; subsequent fields render
/// `"  {label}:"` (two-space prefix).
fn field_row_children(
    prefix: &str,
    row_fields: &[FilterFieldView],
    rc: ResolvedColors,
) -> Vec<AnyElement<'static>> {
    let mut children: Vec<AnyElement<'static>> = Vec::new();
    children.push(
        element! {
            Text(content: prefix, color: rc.dim)
        }
        .into_any(),
    );
    for (i, field) in row_fields.iter().enumerate() {
        let mut view = field.clone();
        if i > 0 {
            view.label = format!("  {}", field.label);
        }
        children.extend(field_group(&view, rc));
    }
    children
}

/// Build a single field-row `Box(height: 1u32)` from the projected fields.
fn field_row_box(
    prefix: &str,
    row_fields: &[FilterFieldView],
    rc: ResolvedColors,
) -> AnyElement<'static> {
    let children = field_row_children(prefix, row_fields, rc);
    element! {
        Box(height: 1u32) {
            #(children)
        }
    }
    .into_any()
}

/// Build the action-hints row: one `Text` per hint segment, all in `rc.dim`.
fn action_hints_row(hints: &[&'static str], rc: ResolvedColors) -> AnyElement<'static> {
    let texts: Vec<AnyElement<'static>> = hints
        .iter()
        .map(|&h| {
            element! {
                Text(content: h, color: rc.dim)
            }
            .into_any()
        })
        .collect();
    element! {
        Box(height: 1u32) {
            #(texts)
        }
    }
    .into_any()
}

/// Build all children for the outer bordered column box: the field rows
/// (chunked into `fields_per_row`), then the action-hints row.
fn outer_children(props: &FilterBarProps, rc: ResolvedColors) -> Vec<AnyElement<'static>> {
    debug_assert!(
        props.fields_per_row > 0,
        "fields_per_row must be > 0 — domain projections must always set it"
    );
    let chunk = props.fields_per_row.max(1);
    let mut children: Vec<AnyElement<'static>> = Vec::new();
    for (row_idx, row_fields) in props.fields.chunks(chunk).enumerate() {
        // Row 0 uses row_prefix; every subsequent row uses continuation_prefix.
        let prefix = if row_idx == 0 {
            props.row_prefix
        } else {
            props.continuation_prefix
        };
        children.push(field_row_box(prefix, row_fields, rc));
    }
    children.push(action_hints_row(&props.action_hints, rc));
    children
}

/// Generic bordered filter bar with labeled `[value]` fields and action hints.
///
/// Renders byte-identically to the pre-refactor `FilterControls` (Issues) and
/// `PrFilterControls` (PRs): a bordered column box → rows of labeled
/// `[value]` fields with active-field inverted-color highlighting → an
/// action-hints row. All field computation, row-prefix text, and action-hint
/// list are supplied by the per-domain projection so this component stays a
/// pure renderer.
#[component]
pub fn FilterBar(props: &FilterBarProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let children = outer_children(props, rc);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            border_style: BorderStyle::Round,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            #(children)
        }
    }
}

/// Build a [`FilterBar`] element from a fully-formed [`FilterBarProps`].
///
/// iocraft's `element!` macro cannot spread a pre-built props struct into a
/// component invocation (each field must be passed inline), so domain wrappers
/// like [`crate::ui::components::issue_filter_props`] return a
/// [`FilterBarProps`] that is rendered through this helper. Screens embed the
/// returned element as a pane child.
#[must_use]
pub fn filter_bar_element(props: FilterBarProps) -> AnyElement<'static> {
    element! {
        FilterBar(
            fields: props.fields,
            visible: props.visible,
            row_prefix: props.row_prefix,
            continuation_prefix: props.continuation_prefix,
            fields_per_row: props.fields_per_row,
            action_hints: props.action_hints,
            colors: props.colors,
        )
    }
    .into_any()
}
