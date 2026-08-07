//! Help modal - keyboard shortcut reference.
//!
//! Renders a scrollable, comprehensive keyboard reference. The content lives
//! in the pure `help_content_lines()` projection (single source of truth); the
//! modal windows it through the shared `ScrollableText` viewport using the
//! `scroll_offset` prop. Scroll actions are applied by the typed action executor
//! (app_input); this component only renders the projection.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::action_projection::{project_help_lines, project_provider_help_lines};
use crate::domain::action_registry::ActionRegistrySnapshot;
use crate::selection::TextSelection;
use crate::theme::{ResolvedColors, ThemeColors};
use crate::ui::components::ScrollableText;

/// Project the complete, ordered keyboard reference from one immutable snapshot.
///
/// Package-contributed actions are appended last (issue #390 CW-10): they did
/// not exist when this binary was built, so they are not in the compiled
/// display table, and an operator who cannot find a package action anywhere in
/// Help has no way to learn it exists or why it will not run.
#[must_use]
pub fn help_content_lines(snapshot: &ActionRegistrySnapshot) -> Vec<String> {
    let mut lines = project_help_lines(snapshot);
    lines.extend(project_provider_help_lines(snapshot));
    lines
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
    /// Active text selection for drag-highlight (issue #178).
    /// Immutable action/binding/availability authority for this render.
    pub action_registry_snapshot: Option<ActionRegistrySnapshot>,
    pub selection: Option<TextSelection>,
}

/// Vertical chrome consumed outside the scroll viewport: border (2) + padding
/// (2) + title (2) + footer (1).
pub const HELP_CHROME_ROWS: u16 = 7;
/// Modal width (columns). Used by both the renderer and the selection geometry.
pub const HELP_MODAL_WIDTH: u16 = 60;
/// Title displayed at the top of the help modal. Used by both the renderer
/// and the selection content projection so they never drift.
pub const HELP_TITLE: &str = "Help - Keyboard Shortcuts";
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

/// The largest scroll offset that still shows the final line of `content`.
///
/// Two things made the tail of Help unreachable, and both are corrected here so
/// the input site and the renderer cannot disagree (issue #390):
///
/// 1. The clamp derived its viewport from the raw terminal size while the modal
///    rendered from the app's render size, which is smaller by the window
///    chrome.
/// 2. More importantly, the offset is in **content-line** units but the
///    viewport shows **wrapped display rows**. Help lines wrap at
///    `HELP_MAX_LINE_WIDTH`, so `lines - viewport` systematically stops short
///    by however many lines happened to wrap — silently, because the modal
///    still looks scrolled to the end.
///
/// The answer is the first content line whose wrapped rows still fill the
/// viewport, computed against the same width the modal renders with.
#[must_use]
pub fn help_max_scroll(content: &[String], terminal_rows: u16) -> usize {
    let render_rows = crate::layout::effective_render_size_for_windowed(0, terminal_rows, true).1;
    let viewport = help_viewport_rows(render_rows);
    let rows = crate::domain::document_wrap::wrap_document(
        &content.join(
            "
",
        ),
        HELP_MAX_LINE_WIDTH,
    );
    let total = rows.len();
    if total <= viewport {
        return 0;
    }
    // The window starts at the first display row of the scrolled-to content
    // line, so it must start at or *after* the row `viewport` from the bottom —
    // an earlier start ends earlier and hides the tail. Scrolling is per
    // content line, so take the first line whose own first row qualifies.
    let target_row = total - viewport;
    rows.iter()
        .skip(target_row)
        .find(|row| row.line_char_start == 0)
        .map_or_else(|| content.len().saturating_sub(1), |row| row.line)
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

    let content = props
        .action_registry_snapshot
        .as_ref()
        .map_or_else(String::new, |snapshot| {
            help_content_lines(snapshot).join("\n")
        });

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
                    content: HELP_TITLE,
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
                    selection: props.selection,
                    selection_bg: Some(rc.sel_bg),
                    selection_fg: Some(rc.sel_fg),
                    content_line_offset: 2usize,
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
        let joined = help_content_lines(&crate::action_projection::test_snapshot()).join("\n");
        // Unified navigation model.
        assert!(
            joined.contains("Switch pane"),
            "must document pane navigation from resolved bindings"
        );
        assert!(
            joined.contains("Focus next / previous detail section"),
            "must document detail subfocus from resolved bindings"
        );
        // Review-thread workflow is discoverable.
        assert!(
            joined.contains("Resolve / unresolve review thread"),
            "must document review-thread resolution from resolved bindings"
        );
        assert!(
            joined.contains("Focus a review thread before resolving or replying"),
            "must document the focus-thread-first resolve flow"
        );
        // Previously-valid actions remain discoverable while chord prefixes come
        // from the effective snapshot and therefore follow user overrides.
        assert!(joined.contains("Grab / move / drop reorder"));
        assert!(joined.contains("Toggle active-only repositories and agents"));
        assert!(joined.contains("Settings"));
        assert!(joined.contains("Open Terminal Manager"));
        assert!(joined.contains("Open / resume or close embedded shell"));
        assert!(joined.contains("Hide embedded shell (keeps it running)"));
        assert!(!joined.contains("F11         Close embedded agent shell"));
        for action in [
            "Open run detail / expand focused job",
            "Expand / collapse focused job",
            "Collapse job, back to runs, then exit Actions",
            "Filter / search workflow runs",
            "Dispatch workflow / refresh runs",
        ] {
            assert!(joined.contains(action), "missing Actions help: {action}");
        }
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

#[cfg(test)]
#[path = "help_scroll_tests.rs"]
mod scroll_tests;
