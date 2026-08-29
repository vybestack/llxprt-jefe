//! Shared rendering shell for host-owned overlay control projections.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for one host-owned overlay projected by a sealed HostControl factory.
#[derive(Default, Props)]
pub struct HostControlOverlayProps {
    /// Operator-facing overlay title.
    pub title: String,
    /// Factory-projected control rows.
    pub rows: Vec<String>,
    /// First visible projected row.
    pub viewport: usize,
    /// Maximum visible projected rows.
    pub viewport_rows: usize,
    /// Overlay width.
    pub width: u32,
    /// Overlay height from the same typed layout used by hit-testing.
    pub height: u32,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Operator-facing footer.
    pub footer: String,
}

/// Render one host-owned overlay without inspecting screen or package identity.
#[component]
pub fn HostControlOverlay(props: &HostControlOverlayProps) -> impl Into<AnyElement<'static>> {
    let colors = ResolvedColors::from_theme(Some(&props.colors));
    let visible: Vec<AnyElement<'static>> = props
        .rows
        .iter()
        .skip(props.viewport)
        .take(props.viewport_rows)
        .map(|row| element! { Text(content: row.clone(), color: colors.fg) }.into_any())
        .collect();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: props.width,
            height: props.height,
            border_style: BorderStyle::Round,
            border_color: colors.border_focused,
            background_color: colors.bg,
            padding: 1u32,
        ) {
            Text(content: props.title.clone(), weight: Weight::Bold, color: colors.fg)
            #(visible)
            Text(content: props.footer.clone(), color: colors.dim)
        }
    }
}
