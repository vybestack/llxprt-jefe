use super::{HostControlRow, HostControlSpan, HostControlSpanRole, PanelHitTarget};
use crate::list_viewport::fit_text_to_width;
use crate::runtime::provider::protocol::{ListItem, ListItemGlyphRole};
use unicode_width::UnicodeWidthStr;

/// One list item's primary row.
///
/// A label plus its trailing suffixes must never wrap: a wrapped sidebar row
/// shifts every later row down and reads as two items (issue #723). The label
/// is the only span this row may elide. A count and a status word are never
/// sliced, because half of one changes what the row says rather than merely
/// shortening it: `Needs you (1…` states a count that is not the count, and
/// `[Runn…` names a status that does not exist (#745).
///
/// The row is exactly one row and always fits `width`. The first of these
/// forms that fits is the one painted, so a suffix is dropped whole rather
/// than cut:
///
/// 1. `marker`, the label fitted to what is left, `" (count)"`, `" [status]"`
///    — the form every shipped pane width renders.
/// 2. the same without the status, which is dropped whole.
/// 3. the same without the count, reachable only when the count cannot share
///    the row with the marker but the status can.
/// 4. `"(count)"`, then `"[status]"` — the marker and the label are sacrificed
///    so one suffix can stay whole. The two together never reach this rung: a
///    count is at least three cells wide, so a row that could hold both bare is
///    already wide enough for rung 3.
/// 5. `marker` and the label fitted to what is left, carrying no suffix. This
///    is also the form an item with neither suffix always takes.
/// 6. the label alone, fitted to the full width, when even the marker does not
///    fit; empty when there is no label either.
pub(super) fn push_list_item_row(
    rows: &mut Vec<HostControlRow>,
    marker: &str,
    item: &ListItem,
    width: usize,
    target: PanelHitTarget,
) {
    let row = match compose_list_item_row(marker, item, width) {
        ListItemRow::Plain(text) => HostControlRow::targeted(text, target),
        ListItemRow::Spanned(spans) => HostControlRow::spanned(spans, Some(target)),
    };
    rows.push(row);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListItemRow {
    Plain(String),
    Spanned(Vec<HostControlSpan>),
}

/// The widest form of a list item's row that fits, per [`push_list_item_row`].
fn compose_list_item_row(marker: &str, item: &ListItem, width: usize) -> ListItemRow {
    let count = item
        .count
        .map_or(String::new(), |value| format!("({value})"));
    let status = item
        .status
        .as_deref()
        .map_or(String::new(), |value| format!("[{value}]"));
    if item.glyph.is_none() && item.badge.is_none() && item.suffix.is_none() {
        return ListItemRow::Plain(compose_plain_list_item_row(
            marker,
            &item.label,
            &count,
            &status,
            width,
        ));
    }
    compose_spanned_list_item_row(marker, item, &count, &status, width)
}

fn compose_spanned_list_item_row(
    marker: &str,
    item: &ListItem,
    count: &str,
    status: &str,
    width: usize,
) -> ListItemRow {
    let joined = join_row_suffixes(count, status);
    let count_only = join_row_suffixes(count, "");
    let status_only = join_row_suffixes("", status);
    let dim_suffix = item.suffix.as_deref().unwrap_or_default();
    let trailing = [
        (joined.as_str(), dim_suffix),
        (joined.as_str(), ""),
        (count_only.as_str(), ""),
        (status_only.as_str(), ""),
    ];
    for (index, (themed, dim)) in trailing.into_iter().enumerate() {
        if (themed.is_empty() && dim.is_empty()) || (index == 0 && dim.is_empty()) {
            continue;
        }
        if let Some(spans) = labelled_spans(marker, item, themed, dim, width) {
            return ListItemRow::Spanned(spans);
        }
    }
    for bare in [count, status] {
        if !bare.is_empty() && UnicodeWidthStr::width(bare) <= width {
            return ListItemRow::Plain(bare.to_owned());
        }
    }
    labelled_spans(marker, item, "", "", width).map_or_else(
        || ListItemRow::Plain(fit_text_to_width(&item.label, width)),
        ListItemRow::Spanned,
    )
}

fn labelled_spans(
    marker: &str,
    item: &ListItem,
    themed_suffix: &str,
    dim_suffix: &str,
    width: usize,
) -> Option<Vec<HostControlSpan>> {
    let badge = item
        .badge
        .as_deref()
        .map_or(String::new(), |badge| format!("[{badge}] "));
    let glyph_width = item.glyph.as_ref().map_or(0, |glyph| {
        UnicodeWidthStr::width(glyph.text.as_str()).saturating_add(1)
    });
    let reserved = UnicodeWidthStr::width(marker)
        .saturating_add(glyph_width)
        .saturating_add(UnicodeWidthStr::width(badge.as_str()))
        .saturating_add(UnicodeWidthStr::width(themed_suffix))
        .saturating_add(UnicodeWidthStr::width(dim_suffix));
    let label_budget = width.checked_sub(reserved)?;
    let mut spans = vec![HostControlSpan {
        text: marker.to_owned(),
        role: HostControlSpanRole::Themed,
    }];
    if let Some(glyph) = &item.glyph {
        spans.push(HostControlSpan {
            text: glyph.text.clone(),
            role: glyph_role(glyph.role),
        });
    }
    let glyph_gap = if item.glyph.is_some() { " " } else { "" };
    spans.push(HostControlSpan {
        text: format!(
            "{glyph_gap}{badge}{}{themed_suffix}",
            fit_text_to_width(&item.label, label_budget)
        ),
        role: HostControlSpanRole::Themed,
    });
    if !dim_suffix.is_empty() {
        spans.push(HostControlSpan {
            text: dim_suffix.to_owned(),
            role: HostControlSpanRole::Dim,
        });
    }
    Some(spans)
}

const fn glyph_role(role: ListItemGlyphRole) -> HostControlSpanRole {
    match role {
        ListItemGlyphRole::Bright => HostControlSpanRole::Bright,
        ListItemGlyphRole::Dim => HostControlSpanRole::Dim,
        ListItemGlyphRole::Red => HostControlSpanRole::Red,
        ListItemGlyphRole::Yellow => HostControlSpanRole::Yellow,
        ListItemGlyphRole::Blue => HostControlSpanRole::Blue,
    }
}

fn compose_plain_list_item_row(
    marker: &str,
    label: &str,
    count: &str,
    status: &str,
    width: usize,
) -> String {
    let suffixes = [
        join_row_suffixes(count, status),
        join_row_suffixes(count, ""),
        join_row_suffixes("", status),
    ];
    for suffix in suffixes.iter().filter(|suffix| !suffix.is_empty()) {
        if let Some(row) = labelled_row(marker, label, suffix, width) {
            return row;
        }
    }
    for bare in [count, status] {
        if !bare.is_empty() && UnicodeWidthStr::width(bare) <= width {
            return bare.to_owned();
        }
    }
    labelled_row(marker, label, "", width).unwrap_or_else(|| fit_text_to_width(label, width))
}

/// The marker, the fitted label and `suffix`, or `None` when the marker and
/// the suffix alone already exceed the row and the label has no room at all.
fn labelled_row(marker: &str, label: &str, suffix: &str, width: usize) -> Option<String> {
    let reserved = UnicodeWidthStr::width(marker) + UnicodeWidthStr::width(suffix);
    let budget = width.checked_sub(reserved)?;
    Some(format!(
        "{marker}{}{suffix}",
        fit_text_to_width(label, budget)
    ))
}

/// The trailing suffixes in their pinned order, each preceded by one space and
/// each omitted when empty.
fn join_row_suffixes(count: &str, status: &str) -> String {
    let mut joined = String::new();
    for token in [count, status] {
        if !token.is_empty() {
            joined.push(' ');
            joined.push_str(token);
        }
    }
    joined
}
