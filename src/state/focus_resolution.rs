//! The focus authority for the shipped host-driven screens (issue #731).
//!
//! Two focus notions exist. [`AppState::pane_focus`] is what the dashboard and
//! split keyboards write (`r`/`a`/`t`/Tab/Left/Right) and what arrow routing,
//! paging and persistence already read. `ScreenInstance::panel_focus` is what
//! the shared screen runtime writes for provider and package screens, through
//! `cycle_panel_focus` and provider mouse routing.
//!
//! `core.dashboard` and `core.repositories` are builtin screens with no
//! package panel bindings, so nothing ever writes their instance focus:
//! `cycle_panel_focus` is gated on a binding builtins never have, and a click
//! on a host-owned panel diverts before the focus write. Mirroring
//! `PaneFocus` into the instance would therefore mean 25 assignment sites each
//! having to remember to update a second field, and one that forgot would be a
//! silent wrong-border bug. Deriving the focused panel here instead leaves one
//! authority per screen, and the renderer, the hit targets and the page
//! geometry all ask the same question of the same field.

use crate::workbench::{
    DASHBOARD_IDENTITY, PanelId, REPOSITORIES_IDENTITY, ResolvedLayout, ScreenDescriptor,
};

use super::{AppState, PaneFocus};

/// Whether this screen's focus authority is the host keyboard's [`PaneFocus`].
fn is_host_driven(descriptor: &ScreenDescriptor) -> bool {
    descriptor.id == DASHBOARD_IDENTITY || descriptor.id == REPOSITORIES_IDENTITY
}

/// [`PaneFocus`] as an ordinal within a screen's declared traversal.
const fn pane_focus_position(focus: PaneFocus) -> usize {
    match focus {
        PaneFocus::Repositories => 0,
        PaneFocus::Agents => 1,
        PaneFocus::Terminal => 2,
    }
}

/// The panel that holds focus on `descriptor` for the frame `layout` describes.
///
/// On a host-driven screen the answer is the declared `focus_order` entry at
/// the pane's ordinal, filtered to the panels this frame actually resolved
/// visible — the same filter `cycle_panel_focus` applies. The dashboard
/// declares the agent list and its zero-agent stand-in side by side
/// (#734/#736) and shows exactly one of them, so its visible traversal is the
/// three panes `PaneFocus` names. Any other screen keeps the per-instance
/// focus its own runtime writes.
#[must_use]
pub fn resolve_focused_panel(
    state: &AppState,
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
) -> PanelId {
    host_driven_focus(state.pane_focus, descriptor, layout)
        .unwrap_or_else(|| state.nav.current().panel_focus)
}

fn host_driven_focus(
    pane_focus: PaneFocus,
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
) -> Option<PanelId> {
    if !is_host_driven(descriptor) {
        return None;
    }
    let visible: Vec<PanelId> = descriptor
        .focus_order
        .iter()
        .filter(|id| layout.panel(id).is_some_and(|panel| panel.visible))
        .copied()
        .collect();
    // A collapsed pane shortens the traversal; the ordinal then clamps to the
    // last pane on screen so focus never names a panel the frame does not draw.
    visible
        .get(pane_focus_position(pane_focus))
        .or_else(|| visible.last())
        .copied()
}

impl AppState {
    /// The panel that holds focus on the current screen, for this frame.
    ///
    /// Falls back to the per-instance focus while the active screen has no
    /// published descriptor or no committed geometry of its own: without a
    /// resolved layout there is no visible traversal to index, and there is
    /// nothing on screen to mark either.
    #[must_use]
    pub fn focused_panel(&self) -> PanelId {
        let current = self.nav.current();
        let Some(descriptor) = self
            .published_workbench()
            .screen_registry()
            .get_identity(current.screen)
        else {
            return current.panel_focus;
        };
        let Some(layout) = self
            .resolved_layout
            .as_ref()
            .filter(|layout| layout.screen_instance == current.id)
        else {
            return current.panel_focus;
        };
        resolve_focused_panel(self, descriptor, layout)
    }
}
