//! Shared scrollable list panel for compact, selection-driven lists.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Single styled text segment in a list panel row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPanelSegment {
    pub text: String,
    pub color: Option<Color>,
}

/// Single display row in a list panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPanelRow {
    pub primary: Vec<ListPanelSegment>,
    pub secondary: Vec<ListPanelSegment>,
}

/// Props for the shared list panel component.
#[derive(Default, Props)]
#[allow(clippy::struct_excessive_bools)]
pub struct ListPanelProps {
    /// Panel title shown in the header row.
    pub title: String,
    /// Rendered rows.
    pub rows: Vec<ListPanelRow>,
    /// Currently selected row index.
    pub selected_index: Option<usize>,
    /// Whether the panel is focused.
    pub focused: bool,
    /// Whether the panel is loading.
    pub loading: bool,
    /// Loading-state message shown while rows are being fetched.
    pub loading_message: String,
    /// Empty-state message when no rows are available.
    pub empty_message: String,
    /// Whether to render only the primary line for selected/unselected items.
    pub compact: bool,
    /// Top row offset for windowing the list content.
    pub scroll_offset: usize,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Shared list panel with clipping-safe viewporting and selection-follow support.
#[component]
pub fn ListPanel(props: &ListPanelProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    let row_height = if props.compact { 1usize } else { 2usize };
    let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
    let viewport_rows = term_rows.saturating_sub(3).max(1);
    let max_visible_items = (viewport_rows / row_height).max(1);
    let total_items = props.rows.len();
    let offset = if total_items == 0 {
        0
    } else {
        props.scroll_offset.min(total_items - 1)
    };
    let visible_rows: Vec<(usize, &ListPanelRow)> = props
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(max_visible_items)
        .collect();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: props.title.clone(), weight: Weight::Bold, color: rc.fg)
            }

            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                background_color: rc.bg,
            ) {
                #(if props.loading {
                    let loading_message = if props.loading_message.is_empty() {
                        "Loading..."
                    } else {
                        props.loading_message.as_str()
                    };
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: loading_message, color: rc.dim)
                        }
                    }]
                } else if props.rows.is_empty() {
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: props.empty_message.clone(), color: rc.dim)
                        }
                    }]
                } else {
                    visible_rows.iter().map(|(i, row)| {
                        let selected = props.selected_index == Some(*i);
                        let primary_text = row
                            .primary
                            .iter()
                            .map(|segment| segment.text.as_str())
                            .collect::<String>();
                        let secondary_text = row
                            .secondary
                            .iter()
                            .map(|segment| segment.text.as_str())
                            .collect::<String>();
                        let primary_color = if selected {
                            rc.sel_fg
                        } else {
                            row.primary
                                .iter()
                                .find_map(|segment| segment.color)
                                .unwrap_or(rc.fg)
                        };
                        let secondary_color = if selected {
                            rc.sel_fg
                        } else {
                            row.secondary
                                .iter()
                                .find_map(|segment| segment.color)
                                .unwrap_or(rc.dim)
                        };
                        if selected {
                            if props.compact {
                                element! {
                                    Box(height: 1u32, background_color: rc.sel_bg) {
                                        Text(
                                            content: primary_text,
                                            color: primary_color,
                                            weight: Weight::Bold,
                                        )
                                    }
                                }
                            } else {
                                element! {
                                    Box(flex_direction: FlexDirection::Column) {
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(
                                                content: primary_text,
                                                color: primary_color,
                                                weight: Weight::Bold,
                                            )
                                        }
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(
                                                content: secondary_text,
                                                color: secondary_color,
                                            )
                                        }
                                    }
                                }
                            }
                        } else if props.compact {
                            element! {
                                Box(height: 1u32) {
                                    Text(content: primary_text, color: primary_color)
                                }
                            }
                        } else {
                            element! {
                                Box(flex_direction: FlexDirection::Column) {
                                    Box(height: 1u32) {
                                        Text(content: primary_text, color: primary_color)
                                    }
                                    Box(height: 1u32) {
                                        Text(
                                            content: secondary_text,
                                            color: secondary_color,
                                        )
                                    }
                                }
                            }
                        }
                    }).collect()
                })
            }
        }
    }
}
