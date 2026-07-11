//! Pure deterministic policy helpers for the terminal scrollback viewport
//! (issue #198).
//!
//! These functions are the single source of truth for scroll-offset math. They
//! are pure (no I/O, no side effects) and `#[must_use]` so they can be
//! unit-tested directly. The reducer calls them on new `AppEvent` variants;
//! the app-shell input layer translates keys/wheel into those events.
//!
//! ## Offset semantics
//!
//! - `None` = **follow-tail** (live): render the bottom of history+live.
//! - `Some(n)` = scrolled back `n` lines from the bottom.
//!
//! The maximum offset is `max_offset = total_lines.saturating_sub(viewport_rows)`,
//! where `total_lines = history_lines + live_rows`. Scrolling down past 0
//! resumes follow-tail by returning `None`.

/// Default single-line scroll step (one wheel tick / arrow).
pub const SCROLL_STEP_LINE: usize = 1;

/// Descriptor for the follow indicator shown when scrolled back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowIndicator {
    /// The raw offset in lines from the bottom.
    pub offset_lines: usize,
    /// The display text (no emoji, per display-and-ui policy).
    pub text: String,
}

/// Compute the maximum valid scroll offset for the given content geometry.
///
/// `total_lines` is `history_lines + live_rows`. The viewport can scroll back
/// until the top of the content reaches the top of the viewport.
#[must_use]
pub fn max_scroll_offset(total_lines: usize, viewport_rows: usize) -> usize {
    total_lines.saturating_sub(viewport_rows)
}

/// Scroll the viewport up (back in history) by `step` lines.
///
/// Returns the new offset. Starting from `None` (follow-tail), the first
/// scroll-up enters scrolled mode at `step` (clamped to `max`). The offset is
/// clamped to `[0, max]`.
#[must_use]
pub fn terminal_scroll_up(
    offset: Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
    step: usize,
) -> Option<usize> {
    let max = max_scroll_offset(total_lines, viewport_rows);
    if max == 0 {
        return None;
    }
    let current = offset.unwrap_or(0);
    let next = current.saturating_add(step).min(max);
    Some(next)
}

/// Scroll the viewport down (toward live) by `step` lines.
///
/// Returns `None` when the viewport reaches the bottom (resume follow-tail by
/// clearing the offset). If already at `None` (follow-tail), stays `None`.
#[must_use]
pub fn terminal_scroll_down(
    offset: Option<usize>,
    _total_lines: usize,
    _viewport_rows: usize,
    step: usize,
) -> Option<usize> {
    let current = offset?;
    if current <= step {
        return None;
    }
    Some(current - step)
}

/// Scroll up by a full page (`viewport_rows` lines).
#[must_use]
pub fn terminal_scroll_page_up(
    offset: Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
) -> Option<usize> {
    terminal_scroll_up(offset, total_lines, viewport_rows, viewport_rows)
}

/// Scroll down by a full page (`viewport_rows` lines).
#[must_use]
pub fn terminal_scroll_page_down(
    offset: Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
) -> Option<usize> {
    terminal_scroll_down(offset, total_lines, viewport_rows, viewport_rows)
}

/// Whether the viewport is currently at the bottom (follow-tail position).
#[must_use]
pub fn terminal_at_bottom(offset: Option<usize>) -> bool {
    offset.is_none()
}

/// Which scrollback direction a terminal scroll event requests.
///
/// Owned by the pure module so the reducer does not need to duplicate the
/// event-to-direction match (which would push it over the clippy line budget).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollRequest {
    Up,
    Down,
    PageUp,
    PageDown,
    FollowTail,
    /// Jump to the top of history (max scroll offset). Home key (issue #198
    /// review fix #8).
    ToTop,
}

/// Reconcile a bottom-relative scroll offset when new content is appended,
/// preserving the viewport's absolute position in the content (issue #198
/// review fix #3).
///
/// When the terminal is scrolled back (`Some(old_offset)`) and new lines are
/// appended, the "bottom" of the content moves down. To keep the viewport
/// showing the same absolute lines, the bottom-relative offset must grow by the
/// number of newly appended lines (`new_total - old_total`), clamped to the new
/// maximum offset.
///
/// Returns `None` when:
/// - Currently following (tail) (`old_offset` is `None`) — follow-tail is
///   unaffected by new content.
/// - No new lines were appended (`new_total <= old_total`).
///
/// Parameters:
/// - `old_offset`: The bottom-relative offset before the content grew.
/// - `old_total_lines`: Total content lines before the growth.
/// - `new_total_lines`: Total content lines after the growth.
/// - `viewport_rows`: The viewport height (for computing the new max).
#[must_use]
pub fn reconcile_offset_for_new_content(
    old_offset: Option<usize>,
    old_total_lines: usize,
    new_total_lines: usize,
    viewport_rows: usize,
) -> Option<usize> {
    let old_offset = old_offset?;
    let delta = new_total_lines.checked_sub(old_total_lines)?;
    if delta == 0 {
        return Some(old_offset);
    }
    let new_max = max_scroll_offset(new_total_lines, viewport_rows);
    Some((old_offset + delta).min(new_max))
}

/// Derive the top-relative content start line for the terminal viewport from a
/// bottom-relative offset (issue #198 review fix #4).
///
/// The viewport projection composes history + live rows into a flat array and
/// windows it starting at `start_line`. When the offset is bottom-relative
/// (Some(n) means n lines up from the bottom), the top-relative start line is:
///
/// ```text
/// start_line = max_offset - bottom_relative_offset
/// ```
///
/// where `max_offset = total_lines - viewport_rows`. This is the single source
/// of truth shared by the viewport projection and the mouse-selection offset
/// so their coordinate systems always agree.
///
/// Returns `0` when the content fits the viewport (max_offset = 0).
#[must_use]
pub fn terminal_content_start_line(
    offset: Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
) -> usize {
    let max_offset = max_scroll_offset(total_lines, viewport_rows);
    let bottom_relative = offset.unwrap_or(0);
    max_offset.saturating_sub(bottom_relative)
}

/// Apply a [`ScrollRequest`] to the current offset, returning the next offset.
///
/// Pure single source of truth: the reducer feeds it the cached geometry and
/// the requested direction; all clamping/follow-tail logic lives here.
#[must_use]
pub fn apply_scroll_request(
    offset: Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
    request: ScrollRequest,
) -> Option<usize> {
    match request {
        ScrollRequest::Up => {
            terminal_scroll_up(offset, total_lines, viewport_rows, SCROLL_STEP_LINE)
        }
        ScrollRequest::Down => {
            terminal_scroll_down(offset, total_lines, viewport_rows, SCROLL_STEP_LINE)
        }
        ScrollRequest::PageUp => terminal_scroll_page_up(offset, total_lines, viewport_rows),
        ScrollRequest::PageDown => terminal_scroll_page_down(offset, total_lines, viewport_rows),
        ScrollRequest::FollowTail => None,
        ScrollRequest::ToTop => {
            let max = max_scroll_offset(total_lines, viewport_rows);
            if max == 0 { None } else { Some(max) }
        }
    }
}

/// Build the follow indicator descriptor when scrolled back.
///
/// Returns `None` when following (offset is `None`). Otherwise returns a
/// [`FollowIndicator`] with the offset and display text. The text contains no
/// emoji (per the display-and-ui policy).
#[must_use]
pub fn terminal_follow_indicator(offset: Option<usize>) -> Option<FollowIndicator> {
    let n = offset?;
    Some(FollowIndicator {
        offset_lines: n,
        text: format!("scrollback: {n} lines up -- End to follow"),
    })
}

/// Map a terminal-scroll `UiNavigationMessage` to a [`ScrollRequest`] and apply
/// it, writing the resulting offset back through `offset`.
///
/// `message` values that are not terminal-scroll messages are a no-op. This
/// keeps the message→request translation in the pure policy module so the
/// reducer stays small.
pub fn apply_terminal_scroll_message(
    offset: &mut Option<usize>,
    total_lines: usize,
    viewport_rows: usize,
    message: crate::messages::UiNavigationMessage,
) {
    let Some(request) = scroll_request_for_message(message) else {
        return;
    };
    *offset = apply_scroll_request(*offset, total_lines, viewport_rows, request);
}

/// Translate a terminal-scroll message into its [`ScrollRequest`], or `None`
/// for non-scroll messages.
fn scroll_request_for_message(
    message: crate::messages::UiNavigationMessage,
) -> Option<ScrollRequest> {
    use crate::messages::UiNavigationMessage as M;
    Some(match message {
        M::TerminalScrollUp => ScrollRequest::Up,
        M::TerminalScrollDown => ScrollRequest::Down,
        M::TerminalScrollPageUp => ScrollRequest::PageUp,
        M::TerminalScrollPageDown => ScrollRequest::PageDown,
        M::TerminalFollowTail => ScrollRequest::FollowTail,
        M::TerminalScrollToTop => ScrollRequest::ToTop,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "scrollback_ops_tests.rs"]
mod tests;
