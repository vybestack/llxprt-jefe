//! Thin iocraft renderer for lowered host-rendered provider screens.
//!
//! Geometry comes exclusively from the frame-owned `state.resolved_layout`;
//! every visible panel is drawn at its resolved chrome/content rectangle.

use iocraft::prelude::*;

use crate::provider_panel_view::{PanelProjection, PanelStatus, project_provider_screen};
use crate::state::AppState;
use crate::theme::{ResolvedColors, ThemeColors};
use crate::workbench::{Rect, screen_registry};

/// Props for a descriptor-driven provider screen.
#[derive(Default, Props)]
pub struct ProviderScreenProps {
    /// Immutable state snapshot for this frame.
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Render the current lowered screen from its descriptor and resolved layout.
#[component]
pub fn ProviderScreen(props: &ProviderScreenProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let Some(state) = props.state.as_ref() else {
        return empty_screen();
    };
    let Ok(registry) = screen_registry() else {
        return empty_screen();
    };
    let Some(descriptor) = registry.get_identity(state.screen()) else {
        return empty_screen();
    };
    let Some(layout) = state.resolved_layout.as_ref() else {
        return empty_screen();
    };
    let view = project_provider_screen(
        descriptor,
        state.nav.current().id.get(),
        &state.provider_panels,
        layout,
        &state.nav.current().panel_focus,
    );
    let mut children = global_chrome(&view.title, layout.outer, &rc);
    if view.too_small {
        children.push(absolute_text(
            layout.outer,
            "screen too small".to_owned(),
            rc.dim,
        ));
    } else {
        for panel in view.panels.iter().filter(|panel| panel.visible) {
            render_panel(panel, &rc, &mut children);
        }
    }

    element! {
        Box(
            position: Position::Relative,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            #(children)
        }
    }
    .into_any()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlobalChromeRects {
    title: Rect,
    footer: Rect,
}

fn global_chrome_rects(outer: Rect) -> GlobalChromeRects {
    GlobalChromeRects {
        title: Rect {
            col: outer.col,
            row: outer.row.saturating_sub(1),
            width: outer.width,
            height: u16::from(outer.row > 0),
        },
        footer: Rect {
            col: outer.col,
            row: outer.row.saturating_add(outer.height),
            width: outer.width,
            height: 1,
        },
    }
}

fn global_chrome(title: &str, outer: Rect, rc: &ResolvedColors) -> Vec<AnyElement<'static>> {
    let rects = global_chrome_rects(outer);
    vec![
        absolute_text(rects.title, title.to_owned(), rc.bright),
        absolute_text(rects.footer, "Esc Back  Ctrl+Q Exit".to_owned(), rc.dim),
    ]
}

fn render_panel(
    panel: &PanelProjection,
    rc: &ResolvedColors,
    children: &mut Vec<AnyElement<'static>>,
) {
    let border_color = match panel.status {
        PanelStatus::Failed => rc.error,
        _ if panel.focused => rc.border_focused,
        _ => rc.border,
    };
    let chrome = panel.chrome;
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(chrome.col),
                top: u32::from(chrome.row),
                width: u32::from(chrome.width),
                height: u32::from(chrome.height),
                border_style: BorderStyle::Round,
                border_color,
            ) {}
        }
        .into_any(),
    );
    let title = panel_title(panel);
    let title_rect = panel_title_rect(chrome, &title);
    children.push(absolute_text(title_rect, title, border_color));

    let rows = panel
        .lines
        .iter()
        .cloned()
        .map(|line| element! { Text(content: line, color: rc.fg) })
        .collect::<Vec<_>>();
    let content = panel.content;
    children.push(
        element! {
            Box(
                position: Position::Absolute,
                left: u32::from(content.col),
                top: u32::from(content.row),
                width: u32::from(content.width),
                height: u32::from(content.height),
                flex_direction: FlexDirection::Column,
            ) {
                #(rows)
            }
        }
        .into_any(),
    );
}
fn panel_title(panel: &PanelProjection) -> String {
    if panel.focused {
        format!(" ▶ {} ", panel.title)
    } else {
        format!(" {} ", panel.title)
    }
}

fn panel_title_rect(chrome: Rect, title: &str) -> Rect {
    let width = u16::try_from(title.chars().count())
        .unwrap_or(u16::MAX)
        .min(chrome.width.saturating_sub(2));
    Rect {
        col: chrome.col.saturating_add(1),
        row: chrome.row,
        width,
        height: u16::from(width > 0),
    }
}

fn absolute_text(rect: Rect, content: String, color: Color) -> AnyElement<'static> {
    element! {
        Box(
            position: Position::Absolute,
            left: u32::from(rect.col),
            top: u32::from(rect.row),
            width: u32::from(rect.width),
            height: u32::from(rect.height),
        ) {
            Text(content: content, color: color)
        }
    }
    .into_any()
}

fn empty_screen() -> AnyElement<'static> {
    element! { Box(width: 0u32, height: 0u32) {} }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_panel_view::{PanelHitTarget, PanelStatus};
    use crate::workbench::PanelId;

    fn projection(focused: bool, chrome: Rect, content: Rect) -> PanelProjection {
        PanelProjection {
            id: PanelId::from_static("main"),
            title: "Main".to_owned(),
            visible: true,
            focused,
            chrome,
            content,
            status: PanelStatus::Active,
            lines: vec!["content".to_owned()],
            max_scroll_offset: 0,
            hit_targets: vec![None::<PanelHitTarget>],
        }
    }

    #[test]
    fn global_title_and_footer_use_the_resolved_screen_bands() {
        let outer = Rect::new(0, 1, 100, 28);

        let rects = global_chrome_rects(outer);

        assert_eq!(rects.title, Rect::new(0, 0, 100, 1));
        assert_eq!(rects.footer, Rect::new(0, 29, 100, 1));
        assert!(!rects.title.contains(0, outer.row));
        assert!(!rects.footer.contains(0, outer.row + outer.height - 1));
    }

    #[test]
    fn focused_panel_title_and_content_preserve_resolved_coordinates() {
        let chrome = Rect::new(2, 3, 20, 10);
        let content = Rect::new(3, 5, 18, 7);
        let panel = projection(true, chrome, content);
        let title = panel_title(&panel);

        assert_eq!(title, " ▶ Main ");
        assert_eq!(panel_title_rect(chrome, &title), Rect::new(3, 3, 8, 1));
        assert_eq!(panel.chrome, chrome);
        assert_eq!(panel.content, content);
        assert!(!panel_title_rect(chrome, &title).contains(content.col, content.row));
    }

    #[test]
    fn panel_title_is_clipped_inside_narrow_resolved_chrome() {
        let chrome = Rect::new(7, 4, 5, 3);
        let panel = projection(false, chrome, Rect::new(8, 5, 3, 1));
        let title = panel_title(&panel);

        assert_eq!(panel_title_rect(chrome, &title), Rect::new(8, 4, 3, 1));
    }
}
