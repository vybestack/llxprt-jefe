//! Close-reason chooser overlay (issue #188).
//!
//! Mirrors `merge_chooser.rs`. Lists the four close reasons with selection
//! markers, shows a duplicate-search input + filtered candidate list when in
//! duplicate mode, and shows a confirm hint.

use iocraft::prelude::*;

use crate::domain::CLOSE_REASONS;
use crate::selection::{SelectablePane, TextSelection};
use crate::state::filter_duplicate_candidates;
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the close-reason chooser overlay.
#[derive(Default, Props)]
pub struct CloseReasonChooserProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Issue number for the header.
    pub issue_number: u64,
    /// 0-based index into `CLOSE_REASONS` for the highlighted reason.
    pub selected_index: usize,
    /// Whether the confirmation step is active.
    pub awaiting_confirmation: bool,
    /// Duplicate search sub-state (query + candidates + selected_index).
    pub duplicate_search_query: Option<String>,
    pub duplicate_candidates: Vec<(u64, String)>,
    pub duplicate_selected_index: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight (issue #178).
    pub selection: Option<TextSelection>,
}

/// Pure projection: build the display lines for the close-reason chooser.
/// Returns `Vec<String>` so it can be unit-tested without iocraft.
#[must_use]
pub fn close_reason_chooser_lines(
    issue_number: u64,
    selected_index: usize,
    awaiting_confirmation: bool,
    duplicate_search_query: Option<&str>,
    duplicate_candidates: &[(u64, String)],
    duplicate_selected_index: usize,
) -> Vec<String> {
    let mut lines = vec![format!("Close Issue #{issue_number}")];
    lines.push(super::SEPARATOR_LINE.to_string());

    for (i, reason) in CLOSE_REASONS.iter().enumerate() {
        let selected = i == selected_index;
        let marker = if selected { "(x)" } else { "( )" };
        lines.push(format!("{} {}", marker, reason.label()));
    }

    if let Some(query) = duplicate_search_query {
        lines.push(super::SEPARATOR_LINE.to_string());
        lines.push(format!("Duplicate of: #{query}"));
        let filtered = filter_duplicate_candidates(duplicate_candidates, query);
        for (i, (num, title)) in filtered.iter().enumerate() {
            let marker = if i == duplicate_selected_index {
                "(x)"
            } else {
                "( )"
            };
            lines.push(format!("{marker} #{num} {title}"));
        }
    }

    lines.push(super::SEPARATOR_LINE.to_string());
    if awaiting_confirmation {
        lines.push("Press Enter to confirm close, Esc to cancel".to_string());
    } else if duplicate_search_query.is_some() {
        lines.push("Type issue #, Up/Down select, Enter confirm, Esc cancel".to_string());
    } else {
        lines.push("Up/Down select  Enter choose  Esc cancel".to_string());
    }
    lines
}

/// Close-reason chooser overlay component.
#[component]
pub fn CloseReasonChooser(props: &CloseReasonChooserProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::CloseReasonChooser;
    let selection = props.selection;
    let mut line_idx: usize = 0;

    let dup_query = props.duplicate_search_query.as_deref();
    let filtered = if let Some(q) = dup_query {
        filter_duplicate_candidates(&props.duplicate_candidates, q)
    } else {
        Vec::new()
    };

    let mut lines: Vec<AnyElement<'static>> = Vec::new();

    // Header + separator
    lines.push(selectable_line(
        &format!("Close Issue #{}", props.issue_number),
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.bright,
        sel,
    ));
    lines.push(selectable_line(
        super::SEPARATOR_LINE,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.dim,
        sel,
    ));

    // Reason list
    for (i, reason) in CLOSE_REASONS.iter().enumerate() {
        let selected = i == props.selected_index;
        let marker = if selected { "(x)" } else { "( )" };
        let label = format!("{marker} {}", reason.label());
        let color = if selected { rc.bright } else { rc.fg };
        lines.push(selectable_line(
            &label,
            {
                let li = line_idx;
                line_idx += 1;
                li
            },
            selection,
            pane,
            color,
            sel,
        ));
    }

    // Duplicate search sub-state
    if let Some(query) = dup_query {
        lines.push(selectable_line(
            super::SEPARATOR_LINE,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.dim,
            sel,
        ));
        lines.push(selectable_line(
            &format!("Duplicate of: #{query}"),
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.bright,
            sel,
        ));
        for (i, (num, title)) in filtered.iter().enumerate() {
            let selected = i == props.duplicate_selected_index;
            let marker = if selected { "(x)" } else { "( )" };
            let label = format!("{marker} #{num} {title}");
            let color = if selected { rc.bright } else { rc.fg };
            lines.push(selectable_line(
                &label,
                {
                    let li = line_idx;
                    line_idx += 1;
                    li
                },
                selection,
                pane,
                color,
                sel,
            ));
        }
    }

    // Separator + hint
    lines.push(selectable_line(
        super::SEPARATOR_LINE,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.dim,
        sel,
    ));
    let hint = if props.awaiting_confirmation {
        "Press Enter to confirm close, Esc to cancel"
    } else if dup_query.is_some() {
        "Type issue #, Up/Down select, Enter confirm, Esc cancel"
    } else {
        "Up/Down select  Enter choose  Esc cancel"
    };
    let hint_color = if props.awaiting_confirmation {
        rc.bright
    } else {
        rc.dim
    };
    lines.push(selectable_line(
        hint, line_idx, selection, pane, hint_color, sel,
    ));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Double,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
            padding_top: 0u32,
            padding_bottom: 0u32,
        ) {
            #(lines)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_for_completed_reason() {
        let lines = close_reason_chooser_lines(42, 0, false, None, &[], 0);
        assert_eq!(lines[0], "Close Issue #42");
        assert!(lines[1].starts_with("───"));
        assert_eq!(lines[2], "(x) Completed");
        assert_eq!(lines[3], "( ) Not planned");
        assert_eq!(lines[4], "( ) Duplicate");
        assert_eq!(lines[5], "( ) Invalid");
        assert!(lines[6].starts_with("───"));
        assert_eq!(lines[7], "Up/Down select  Enter choose  Esc cancel");
    }

    #[test]
    fn lines_for_awaiting_confirmation() {
        let lines = close_reason_chooser_lines(42, 1, true, None, &[], 0);
        let last = lines.last().map(String::as_str).unwrap_or_default();
        assert_eq!(last, "Press Enter to confirm close, Esc to cancel");
    }

    #[test]
    fn lines_for_duplicate_search() {
        let candidates = vec![(10u64, "First".to_string()), (100u64, "Second".to_string())];
        let lines = close_reason_chooser_lines(42, 2, false, Some("1"), &candidates, 0);
        let dup_header_idx = lines
            .iter()
            .position(|l| l.starts_with("Duplicate of:"))
            .unwrap_or_else(|| panic!("must have duplicate header"));
        assert_eq!(lines[dup_header_idx], "Duplicate of: #1");
        // Filtered candidates: both start with "1".
        assert!(lines.iter().any(|l| l.contains("#10")));
        assert!(lines.iter().any(|l| l.contains("#100")));
        // The selection marker must land on the first filtered candidate only.
        assert!(
            lines.iter().any(|l| l.starts_with("(x) #10")),
            "first filtered candidate should be marked selected"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("( ) #100")),
            "second filtered candidate should be marked unselected"
        );
        let last = lines.last().map(String::as_str).unwrap_or_default();
        assert_eq!(
            last,
            "Type issue #, Up/Down select, Enter confirm, Esc cancel"
        );
    }

    /// Parity guard: verify the projection emits exactly the reasons in
    /// `CLOSE_REASONS` order with markers, so the component (which mirrors
    /// this ordering) and the projection cannot silently diverge on which
    /// reasons are shown or their selection state.
    #[test]
    fn projection_lists_all_reasons_in_domain_order_with_markers() {
        let lines = close_reason_chooser_lines(42, 1, false, None, &[], 0);
        // Header + separator + one line per reason + separator + hint.
        let expected_len = 2 + crate::domain::CLOSE_REASONS.len() + 2;
        assert_eq!(lines.len(), expected_len);
        for (i, reason) in crate::domain::CLOSE_REASONS.iter().enumerate() {
            let marker = if i == 1 { "(x)" } else { "( )" };
            let expected = format!("{marker} {}", reason.label());
            assert_eq!(
                lines[2 + i],
                expected,
                "reason at index {i} should carry the right marker and label"
            );
        }
    }

    #[test]
    fn projection_confirmation_hint_wins_over_duplicate_search() {
        // The combination awaiting_confirmation=true + duplicate_search_query
        // is the final-confirm path after picking a duplicate target. The
        // confirmation hint must take precedence so the user knows Enter will
        // commit the close.
        let candidates = vec![(2u64, "Other".to_string())];
        let lines = close_reason_chooser_lines(7, 2, true, Some("2"), &candidates, 0);
        assert!(
            lines
                .iter()
                .any(|l| l == "Press Enter to confirm close, Esc to cancel"),
            "awaiting-confirmation hint must win over duplicate-search hint"
        );
        // The duplicate-search section should still render the resolved target.
        assert!(lines.iter().any(|l| l.contains("Duplicate of: #2")));
    }
}
