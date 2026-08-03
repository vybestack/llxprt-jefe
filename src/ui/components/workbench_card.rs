//! Workbench card component — renders one [`WorkbenchCard`] as a fixed-height
//! bordered box (issue #626).
//!
//! Modeled on [`super::preview::Preview`]: iocraft `#[derive(Props)]` +
//! `#[component]`. The card renders exactly the lines specified by the issue
//! so every card is the same height regardless of content. Selected cards use
//! a double border; unselected cards use a round border.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};
use crate::workbench_view::{StatusBucket, TodoRender, WorkbenchCard as WorkbenchCardModel};

/// Props for the workbench card component.
#[derive(Default, Props)]
pub struct WorkbenchCardProps {
    /// The resolved card view model.
    pub card: Option<WorkbenchCardModel>,
    /// Interior card width (excludes borders).
    pub card_width: usize,
    /// Todo-window size (number of todo lines).
    pub todo_window: usize,
    /// Whether this card is selected (double border vs round border).
    pub selected: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Render one workbench agent card as a fixed-height bordered box.
#[component]
pub fn WorkbenchCard(props: &WorkbenchCardProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.selected {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };
    let Some(card) = props.card.as_ref() else {
        return empty_card(rc, border_style);
    };

    let width = u32::try_from(props.card_width).unwrap_or(u32::MAX);
    let lines = card_lines(card, props.card_width, props.todo_window, &rc);
    let children = lines
        .into_iter()
        .map(|line| {
            element! {
                Box(height: 1u32, width: width, background_color: rc.bg) {
                    Text(content: line.text, color: line.color, wrap: TextWrap::NoWrap)
                }
            }
            .into_any()
        })
        .collect::<Vec<_>>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
            padding: 0u32,
        ) {
            #(children)
        }
    }
    .into_any()
}

/// One rendered line with its color.
struct RenderLine {
    text: String,
    color: Color,
}

/// Build the header line (glyph + label + slot + name + elapsed).
fn header_line(card: &WorkbenchCardModel, card_width: usize, rc: &ResolvedColors) -> RenderLine {
    let glyph = bucket_glyph(card.bucket);
    let slot = card
        .header
        .shortcut_slot
        .as_deref()
        .map(|s| format!(" [{s}]"))
        .unwrap_or_default();
    let header = format!(
        "{glyph} {}{slot} {}  {}",
        card.header.status_label, card.header.repo_name.text, card.header.elapsed
    );
    RenderLine {
        text: pad_to_width(&header, card_width),
        color: bucket_color(card.bucket, rc),
    }
}

/// Build the todo progress header line.
fn todo_header_line(card: &WorkbenchCardModel, card_width: usize, dim: Color) -> RenderLine {
    let header = match &card.todos {
        TodoRender::Known(window) => {
            format!(
                "todos  {}/{} done {}",
                window.done,
                window.total,
                progress_bar(window.done, window.total)
            )
        }
        TodoRender::Unknown => "todos  (unknown)".to_string(),
        TodoRender::Unsupported => "todos  (unsupported)".to_string(),
    };
    RenderLine {
        text: pad_to_width(&header, card_width),
        color: dim,
    }
}

/// Build the todo body lines (exactly `todo_window` entries).
fn todo_body_lines(
    card: &WorkbenchCardModel,
    card_width: usize,
    todo_window: usize,
    fg: Color,
    bright: Color,
) -> Vec<RenderLine> {
    match &card.todos {
        TodoRender::Known(window) => window
            .visible
            .iter()
            .map(|line| {
                if line.is_blank {
                    blank_line(card_width, fg)
                } else {
                    RenderLine {
                        text: pad_to_width(&line.text, card_width),
                        color: if line.is_current { bright } else { fg },
                    }
                }
            })
            .collect(),
        _ => (0..todo_window)
            .map(|_| blank_line(card_width, fg))
            .collect(),
    }
}

/// Build the exact fixed set of lines for a card.
fn card_lines(
    card: &WorkbenchCardModel,
    card_width: usize,
    todo_window: usize,
    rc: &ResolvedColors,
) -> Vec<RenderLine> {
    let fg = rc.fg;
    let dim = rc.dim;
    let bright = rc.bright;

    let mut lines = Vec::with_capacity(6 + todo_window);
    lines.push(header_line(card, card_width, rc));
    lines.push(RenderLine {
        text: pad_to_width(&card.need, card_width),
        color: fg,
    });
    lines.push(blank_line(card_width, fg));
    lines.push(todo_header_line(card, card_width, dim));
    lines.extend(todo_body_lines(card, card_width, todo_window, fg, bright));
    lines.push(blank_line(card_width, fg));
    let msg = card.last_message.as_deref().unwrap_or("—");
    lines.push(RenderLine {
        text: pad_to_width(&format!("⤷ {msg}"), card_width),
        color: dim,
    });
    lines
}

/// A blank line of `width` spaces.
fn blank_line(width: usize, color: Color) -> RenderLine {
    RenderLine {
        text: spaces(width),
        color,
    }
}

/// A single-character glyph for a status bucket.
fn bucket_glyph(bucket: StatusBucket) -> char {
    match bucket {
        StatusBucket::NeedsYou => '●',
        StatusBucket::Working => '◐',
        StatusBucket::Ready => '○',
        StatusBucket::Stale => '◇',
    }
}

/// Pick a color for the header based on bucket priority.
fn bucket_color(bucket: StatusBucket, rc: &ResolvedColors) -> Color {
    match bucket {
        StatusBucket::NeedsYou => rc.bright,
        StatusBucket::Working => rc.fg,
        StatusBucket::Ready | StatusBucket::Stale => rc.dim,
    }
}

/// Build a 10-cell progress bar from done/total.
fn progress_bar(done: usize, total: usize) -> String {
    const CELLS: usize = 10;
    if total == 0 {
        return format!("[{}]", " ".repeat(CELLS));
    }
    let filled = (done * CELLS) / total;
    let empty = CELLS - filled.min(CELLS);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Pad or truncate a string to exactly `width` visible characters.
fn pad_to_width(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() >= width {
        chars[..width].iter().collect()
    } else {
        let pad = width - chars.len();
        let mut s: String = chars.into_iter().collect();
        s.push_str(&" ".repeat(pad));
        s
    }
}

/// A string of `width` spaces.
fn spaces(width: usize) -> String {
    " ".repeat(width)
}

/// Render an empty card placeholder (when no card model is provided).
fn empty_card(rc: ResolvedColors, border_style: BorderStyle) -> AnyElement<'static> {
    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        )
    }
    .into_any()
}
