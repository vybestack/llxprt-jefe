//! Exact terminal dimensions for the current screen-instance frame.

use jefe::state::AppState;
use jefe::workbench::{PanelId, ScreenId};

#[must_use]
pub fn terminal_pane_dimensions(
    snapshot: &AppState,
    term_cols: u16,
    term_rows: u16,
) -> (usize, usize) {
    if snapshot.shell_overlay_active() {
        let layout = if snapshot.screen() == ScreenId::Terminals {
            jefe::layout::compute_terminal_manager_pty_layout(term_cols, term_rows)
        } else {
            jefe::layout::compute_shell_overlay_pty_layout(term_cols, term_rows)
        };
        return (usize::from(layout.pty_rows), usize::from(layout.pty_cols));
    }

    let Some(layout) = snapshot
        .resolved_layout
        .as_ref()
        .filter(|layout| layout.screen_instance == snapshot.nav.current().id)
    else {
        return (0, 0);
    };
    let Some(descriptor) = snapshot
        .published_workbench()
        .screen_registry()
        .get_identity(snapshot.screen())
    else {
        return (0, 0);
    };
    jefe::workbench::pty_content_rect(descriptor, layout, &PanelId::from_static("terminal"))
        .map_or((0, 0), |rect| {
            (usize::from(rect.height), usize::from(rect.width))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::workbench::{
        ActivationValues, LayoutGeneration, Rect, ResolvedLayout, ResolvedPanel, RouteId,
        ScreenInstanceId,
    };

    fn terminal_layout(instance: ScreenInstanceId, content: Rect) -> ResolvedLayout {
        ResolvedLayout {
            screen_instance: instance,
            generation: LayoutGeneration::next(),
            outer: Rect::new(0, 1, 120, 38),
            panels: vec![ResolvedPanel {
                id: PanelId::from_static("terminal"),
                visible: true,
                chrome: content,
                content,
                depth_first_index: 0,
                hit_region: Some(content),
            }],
            too_small: None,
        }
    }

    #[test]
    fn ordinary_terminal_dimensions_require_the_exact_current_instance_layout() {
        let mut state = crate::test_app_state();
        let first = state.nav.current().id;
        let terminal = Rect::new(42, 11, 73, 19);
        state.resolved_layout = Some(terminal_layout(first, terminal));
        assert_eq!(terminal_pane_dimensions(&state, 120, 40), (19, 73));

        state.enter_provider_route(RouteId::from_static("dashboard"), ActivationValues::empty());
        assert_ne!(state.nav.current().id, first);
        assert_eq!(
            terminal_pane_dimensions(&state, 120, 40),
            (0, 0),
            "a suspended instance's frame geometry must not size the current PTY"
        );
    }

    #[test]
    fn ordinary_terminal_dimensions_do_not_rederive_missing_frame_geometry() {
        let state = crate::test_app_state();

        assert_eq!(terminal_pane_dimensions(&state, 120, 40), (0, 0));
    }
}
