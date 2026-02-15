//! Help modal showing keyboard shortcuts.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the help modal.
#[derive(Default, Props)]
pub struct HelpModalProps {
    /// Whether the modal is visible.
    pub visible: bool,
    /// Scroll offset for scrolling through content.
    pub scroll_offset: u32,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Available terminal height for clipping.
    pub height: u32,
}

/// Keyboard shortcut reference modal.
#[component]
pub fn HelpModal(props: &HelpModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    if !props.visible {
        return element! { Box() };
    }

    // Define content sections with headers and shortcuts
    let content_lines: Vec<(String, Color, bool)> = vec![
        ("".to_string(), rc.fg, false),
        ("Navigation".to_string(), rc.bright, true),
        ("  ↑ / ↓         Navigate up / down".to_string(), rc.fg, false),
        ("  ← / →         Switch pane focus".to_string(), rc.fg, false),
        ("  Enter          Select / confirm".to_string(), rc.fg, false),
        ("  Esc            Back / close modal".to_string(), rc.fg, false),
        ("".to_string(), rc.fg, false),
        ("Pane Focus".to_string(), rc.bright, true),
        ("  r              Focus repository sidebar".to_string(), rc.fg, false),
        ("  a              Focus agent list".to_string(), rc.fg, false),
        ("  t              Focus terminal pane".to_string(), rc.fg, false),
        ("  m              Return to main view".to_string(), rc.fg, false),
        ("".to_string(), rc.fg, false),
        ("Terminal".to_string(), rc.bright, true),
        ("  F12            Attach / detach terminal".to_string(), rc.fg, false),
        ("                 (all keys forward when attached)".to_string(), rc.fg, false),
        ("".to_string(), rc.fg, false),
        ("Agent Actions".to_string(), rc.bright, true),
        ("  n              New agent".to_string(), rc.fg, false),
        ("  N              New repository".to_string(), rc.fg, false),
        ("  d              Delete agent".to_string(), rc.fg, false),
        ("  D              Delete repository".to_string(), rc.fg, false),
        ("  k              Kill agent".to_string(), rc.fg, false),
        ("  l              Relaunch agent".to_string(), rc.fg, false),
        ("".to_string(), rc.fg, false),
        ("Views".to_string(), rc.bright, true),
        ("  s              Toggle split mode".to_string(), rc.fg, false),
        ("  /              Search / command palette".to_string(), rc.fg, false),
        ("  ? / h / F1     This help dialog".to_string(), rc.fg, false),
        ("  q              Quit".to_string(), rc.fg, false),
        ("".to_string(), rc.fg, false),
    ];

    let total_lines = content_lines.len() as u32;
    
    // Calculate available height for content (terminal height - border - title - footer)
    let inner_height = props.height.saturating_sub(6);
    let scroll_offset = props.scroll_offset;
    
    // Determine visible range
    let start_idx = scroll_offset as usize;
    let end_idx = (scroll_offset + inner_height).min(total_lines) as usize;
    let visible_lines: Vec<_> = content_lines
        .iter()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .cloned()
        .collect();

    let has_more_below = end_idx < total_lines as usize;
    let has_more_above = scroll_offset > 0;

    // Build footer text
    let footer_text = if has_more_below || has_more_above {
        "  ↑↓ scroll   Esc to close".to_string()
    } else {
        "  Press Esc to close".to_string()
    };

    element! {
        Box(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            background_color: rc.bg,
        ) {
            Box(
                border_style: BorderStyle::Round,
                border_color: rc.border_focused,
                background_color: rc.bg,
                flex_direction: FlexDirection::Column,
                padding: 1i32,
                width: 55u32,
            ) {
                Box(height: 1u32) {
                    Text(content: " Keyboard Shortcuts".to_owned(), color: rc.fg, weight: Weight::Bold)
                }
                Box(height: 1u32) {
                    Text(content: "".to_owned(), color: rc.dim)
                }
                #(visible_lines.into_iter().map(|(line, color, is_header)| {
                    element! {
                        Box(height: 1u32) {
                            Text(
                                content: line,
                                color: color,
                                weight: if is_header { Weight::Bold } else { Weight::Normal }
                            )
                        }
                    }
                }))
                #(if has_more_below {
                    vec![element! {
                        Box(height: 1u32) {
                            Text(content: "  ↓ more below".to_owned(), color: rc.dim)
                        }
                    }]
                } else {
                    vec![element! { Box(height: 1u32) }]
                })
                Box(height: 1u32, padding_top: 1i32) {
                    Text(content: footer_text, color: rc.dim)
                }
            }
        }
    }
}
