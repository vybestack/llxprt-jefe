//! Issues detail scroll/subfocus dispatch helpers.
//!
//! Extracted from `app_input/mod.rs` to keep that file under the 1000-line
//! source-file limit. Both the detail scroll-down and the subfocus-next/prev
//! paths refresh the cached viewport row count before the reducer runs so the
//! scroll clamp and the scroll-into-view computation (#151) use a fresh
//! viewport height.

use jefe::messages::{AppMessage, IssuesMessage};
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, issues_dispatch, update_detail_viewport_rows,
};

/// Dispatch Issues detail scroll and subfocus messages.
///
/// Both paths refresh the cached viewport row count before the reducer runs so
/// the scroll clamp and the scroll-into-view computation (#151) use a fresh
/// viewport height. Scroll-down arms additionally trigger the load-more-comments
/// check.
pub(super) fn dispatch_issues_detail_scroll_or_subfocus(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    let is_scroll_down = matches!(
        message,
        IssuesMessage::ScrollDetailDown | IssuesMessage::ScrollDetailPageDown
    );
    update_detail_viewport_rows(app_state);
    apply_and_persist(app_state, ctx, AppEvent::from(AppMessage::Issues(message)));
    if is_scroll_down {
        issues_dispatch::load_more_comments(app_state, ctx);
    }
}
