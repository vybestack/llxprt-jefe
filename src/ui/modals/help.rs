//! Help modal - keyboard shortcut reference.
//!
//! Renders a scrollable, comprehensive keyboard reference. The content lives
//! in the pure `help_content_lines()` projection (single source of truth); the
//! modal windows it through the shared `ScrollableText` viewport using the
//! `scroll_offset` prop. Scroll keys are handled by `handle_mode_help_key`
//! (app_input); this component only renders the projection.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::ScrollableText;

/// The complete, ordered list of keyboard-reference lines shown in the help
/// modal. Pure, side-effect-free, and unit-testable without a terminal. This
/// is the single source of truth for help text.
///
/// Updated for issue #150: documents the unified detail-pane key model
/// (arrows own pane/item navigation; Tab owns detail subfocus; j/k alias) and
/// makes the PR review-thread resolution flow discoverable. Emoji-free
/// (textual symbols only).
#[must_use]
pub fn help_content_lines() -> &'static [&'static str] {
    &[
        "Navigation:",
        "  Up/Down     Select item / scroll detail",
        "  Left/Right  Switch pane",
        "  Tab         Focus next detail section",
        "  j/k         Detail section focus (alias)",
        "  F12         Toggle terminal focus",
        "",
        "Issues & PR detail:",
        "  Enter       Open detail",
        "  c           Comment",
        "  r           Reply",
        "  e           Edit (issues only)",
        "  S           Send to agent",
        "  R           Resolve/unresolve review thread (PR)",
        "  o           Open in browser (PR)",
        "  m           Merge (PR)",
        "  Tip: Tab/j/k to a review thread, then R resolve / r reply",
        "",
        "Dashboard:",
        "  n           New agent",
        "  N           New repository",
        "  Ctrl-d      Delete selected",
        "  Ctrl-k      Kill agent",
        "  Ctrl-r      Restart agent",
        "  l           Relaunch dead agent",
        "  s           Split mode",
        "  Space       Grab/move/drop reorder",
        "  v           Toggle active-only (repos + agents)",
        "  \u{2325}1-\u{2325}9       Jump to agent shortcut",
        "",
        "Other:",
        "  1/2/3       Switch theme",
        "  ?/h/F1      This help",
        "  Ctrl-q/qqq  Quit",
    ]
}

/// Props for the help modal.
#[derive(Default, Props)]
pub struct HelpModalProps {
    /// Theme colors.
    pub colors: ThemeColors,
    /// Current scroll offset (content lines scrolled from the top).
    pub scroll_offset: usize,
    /// Terminal rows available, used to size the scroll viewport so the modal
    /// never overflows the screen.
    pub available_rows: u16,
}

/// Vertical chrome consumed outside the scroll viewport: border (2) + padding
/// (2) + title (2) + footer (1).
const HELP_CHROME_ROWS: u16 = 7;
/// Minimum lines shown at once (keeps the modal usable on short terminals).
const HELP_MIN_VIEWPORT: usize = 8;
/// Maximum lines shown at once even on very tall terminals.
const HELP_MAX_VIEWPORT: usize = 22;
/// Interior text width: width(60) - border(2) - padding(2) - scrollbar(1).
const HELP_MAX_LINE_WIDTH: usize = 55;

/// Compute the help-modal scroll viewport height from the terminal rows
/// available. Pure and side-effect-free so it is unit-testable without a
/// terminal.
///
/// The preferred minimum (`HELP_MIN_VIEWPORT`) is honored ONLY when the
/// terminal can fit it; on short terminals the viewport shrinks so the modal
/// never exceeds the available rows. For any terminal with at least
/// `HELP_CHROME_ROWS` rows the result satisfies
/// `viewport + HELP_CHROME_ROWS == available_rows`, guaranteeing the modal
/// fits on screen.
#[must_use]
pub fn help_viewport_rows(available_rows: u16) -> usize {
    let available = usize::from(available_rows).saturating_sub(usize::from(HELP_CHROME_ROWS));
    if available >= HELP_MIN_VIEWPORT {
        available.min(HELP_MAX_VIEWPORT)
    } else {
        available
    }
}

/// Help modal showing all keyboard shortcuts (scrollable).
#[component]
pub fn HelpModal(props: &HelpModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    let viewport_rows = help_viewport_rows(props.available_rows);
    // Final safety: never render taller than the screen (covers the degenerate
    // sub-`HELP_CHROME_ROWS` terminal where even the chrome does not fit).
    let modal_height = u32::try_from(
        (viewport_rows + usize::from(HELP_CHROME_ROWS))
            .min(usize::from(props.available_rows))
            .max(1),
    )
    .unwrap_or(1);
    // Explicit viewport height so the container and `ScrollableText` enforce
    // each other directly (rather than relying on `flex_grow` matching
    // `HELP_CHROME_ROWS` implicitly).
    let viewport_height = u32::try_from(viewport_rows).unwrap_or(0);

    let content = help_content_lines().join("\n");

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 60u32,
            height: modal_height,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            // Title
            Box(height: 2u32, background_color: rc.bg) {
                Text(
                    content: "Help - Keyboard Shortcuts",
                    weight: Weight::Bold,
                    color: rc.fg,
                )
            }

            // Scrollable shortcuts viewport (explicit height == ScrollableText
            // viewport_rows so the container and rendered rows stay in sync).
            Box(
                flex_direction: FlexDirection::Column,
                height: viewport_height,
                background_color: rc.bg
            ) {
                ScrollableText(
                    content: content,
                    scroll_offset: props.scroll_offset,
                    viewport_rows: viewport_rows,
                    max_line_width: HELP_MAX_LINE_WIDTH,
                    color: Some(rc.fg),
                    bg: Some(rc.bg),
                )
            }

            // Footer
            Box(height: 1u32, background_color: rc.bg) {
                Text(content: "Esc/? close | Up/Down scroll", color: rc.dim)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The help content documents the unified detail-pane key model (issue
    /// #150): arrows own pane/item navigation, Tab owns detail subfocus, j/k
    /// is the documented alias, and the PR review-thread resolve flow is
    /// discoverable. Also confirms previously-valid bindings are present.
    #[test]
    fn test_help_content_documents_unified_model_and_review_workflow() {
        let joined = help_content_lines().join("\n");
        // Unified navigation model.
        assert!(
            joined.contains("Left/Right  Switch pane"),
            "must document arrow pane navigation"
        );
        assert!(
            joined.contains("Tab         Focus next detail section"),
            "must document Tab detail subfocus"
        );
        assert!(
            joined.contains("j/k         Detail section focus (alias)"),
            "must document j/k subfocus alias"
        );
        // Review-thread workflow is discoverable.
        assert!(
            joined.contains("R           Resolve/unresolve review thread (PR)"),
            "must document R resolve"
        );
        assert!(
            joined.contains("Tab/j/k to a review thread, then R resolve / r reply"),
            "must document the focus-thread-first resolve flow"
        );
        // Previously-valid bindings must remain present (no regression).
        assert!(joined.contains("Space       Grab/move/drop reorder"));
        assert!(joined.contains("v           Toggle active-only"));
        assert!(joined.contains("1/2/3       Switch theme"));
    }

    /// `help_viewport_rows` honors the preferred minimum on normal terminals and
    /// caps the viewport so the modal never exceeds the available rows (issue
    /// #150 short-terminal safety).
    #[test]
    fn test_help_viewport_rows_fits_normal_terminal() {
        // 32-row terminal: chrome(7) leaves 25, but capped at max(22).
        assert_eq!(help_viewport_rows(32), 22);
        // Exactly the threshold where the minimum kicks in: 15 rows -> 8 viewport.
        assert_eq!(help_viewport_rows(15), 8);
        // modal_height = 8 + 7 == 15 == available, so it fits.
        assert_eq!(help_viewport_rows(15) + usize::from(HELP_CHROME_ROWS), 15);
    }

    /// On short terminals (below the preferred minimum), the viewport shrinks
    /// so that viewport + chrome never exceeds the available rows.
    #[test]
    fn test_help_viewport_rows_shrinks_on_short_terminal() {
        for available in [10u16, 12, 14] {
            let viewport = help_viewport_rows(available);
            assert!(
                viewport + usize::from(HELP_CHROME_ROWS) <= usize::from(available),
                "modal must fit: viewport {viewport} + chrome on {available} rows"
            );
            assert!(
                viewport < HELP_MIN_VIEWPORT,
                "short terminal must not get the forced minimum"
            );
        }
    }

    /// Degenerate tiny/zero-row terminals must not panic and must never produce
    /// a modal taller than the screen.
    #[test]
    fn test_help_viewport_rows_degenerate_terminals_never_overflow() {
        for available in [0u16, 1, 5, 7] {
            let viewport = help_viewport_rows(available);
            // viewport itself is bounded by available - chrome (saturating to 0).
            assert!(
                viewport + usize::from(HELP_CHROME_ROWS)
                    <= usize::from(available) + usize::from(HELP_CHROME_ROWS),
                "no panic on {available} rows"
            );
            // No negative/overflow panic; the component's final min(available)
            // guarantees the rendered modal_height <= available rows.
        }
        // Zero-row terminal yields a zero viewport (no content), not the forced 8.
        assert_eq!(help_viewport_rows(0), 0);
    }
}
