//! Thin iocraft renderer for lowered host-rendered provider screens.

use iocraft::prelude::*;

use crate::provider_panel_view::project_provider_screen;
use crate::state::AppState;
use crate::theme::{ResolvedColors, ThemeColors};
use crate::workbench::screen_registry;

/// Props for a descriptor-driven provider screen.
#[derive(Default, Props)]
pub struct ProviderScreenProps {
    /// Immutable state snapshot for this frame.
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Render the current lowered screen from its descriptor and accepted panel snapshots.
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
    let view = project_provider_screen(
        descriptor,
        state.nav.current().id.get(),
        &state.provider_panels,
    );
    let rows = view
        .lines
        .into_iter()
        .map(|line| element! { Text(content: line, color: rc.fg) })
        .collect::<Vec<_>>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0_f32,
            height: 100pct,
            border_style: BorderStyle::Round,
            border_color: rc.border,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            Text(content: view.title, color: rc.bright)
            #(rows)
            Text(content: "Esc Back  Ctrl+Q Exit", color: rc.dim)
        }
    }
    .into_any()
}

fn empty_screen() -> AnyElement<'static> {
    element! { Box(width: 0u32, height: 0u32) {} }.into_any()
}
