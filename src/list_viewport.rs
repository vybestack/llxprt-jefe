//! Pure selectable-list geometry, windowing, navigation, and width fitting.
//!
//! This module intentionally has no terminal-renderer dependency. Screens turn
//! physical pane dimensions into typed geometry here, reducers consume typed
//! page counts, and UI projections consume the same deterministic viewport.

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

macro_rules! row_count_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $name(usize);

        impl $name {
            #[must_use]
            pub const fn new(rows: usize) -> Self {
                Self(rows)
            }

            #[must_use]
            pub const fn get(self) -> usize {
                self.0
            }
        }
    };
}

row_count_type!(PaneRows);
row_count_type!(ContentRows);
row_count_type!(BorderRows);
row_count_type!(TitleRows);
row_count_type!(PaddingRows);
row_count_type!(ItemCapacity);

/// Number of logical items moved by a page-navigation command.
///
/// Zero means the pane has no renderable item rows, so page movement is a
/// no-op rather than moving the selection off-screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PageItemCount(usize);

impl PageItemCount {
    #[must_use]
    pub const fn new(items: usize) -> Self {
        Self(items)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Uniform physical row extent of one logical list item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RowsPerItem(usize);

impl RowsPerItem {
    #[must_use]
    pub const fn new(rows: usize) -> Self {
        Self(if rows == 0 { 1 } else { rows })
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for RowsPerItem {
    fn default() -> Self {
        Self(1)
    }
}

/// Explicit pane-to-content geometry for a uniformly-sized selectable list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListGeometry {
    border_rows: BorderRows,
    title_rows: TitleRows,
    padding_rows: PaddingRows,
    rows_per_item: RowsPerItem,
}

impl ListGeometry {
    /// Standard bordered list with one title row and no content padding.
    #[must_use]
    pub const fn bordered(rows_per_item: RowsPerItem) -> Self {
        Self::new(
            BorderRows::new(2),
            TitleRows::new(1),
            PaddingRows::new(0),
            rows_per_item,
        )
    }

    /// Standard bordered list with one title row and one row of padding on
    /// each vertical edge.
    #[must_use]
    pub const fn bordered_padded(rows_per_item: RowsPerItem) -> Self {
        Self::new(
            BorderRows::new(2),
            TitleRows::new(1),
            PaddingRows::new(2),
            rows_per_item,
        )
    }

    #[must_use]
    pub const fn new(
        border_rows: BorderRows,
        title_rows: TitleRows,
        padding_rows: PaddingRows,
        rows_per_item: RowsPerItem,
    ) -> Self {
        Self {
            border_rows,
            title_rows,
            padding_rows,
            rows_per_item,
        }
    }

    #[must_use]
    pub const fn content_rows(self, pane_rows: PaneRows) -> ContentRows {
        let chrome = self
            .border_rows
            .get()
            .saturating_add(self.title_rows.get())
            .saturating_add(self.padding_rows.get());
        ContentRows::new(pane_rows.get().saturating_sub(chrome))
    }

    #[must_use]
    pub const fn item_capacity(self, pane_rows: PaneRows) -> ItemCapacity {
        ItemCapacity::new(self.content_rows(pane_rows).get() / self.rows_per_item.get())
    }

    #[must_use]
    pub const fn rows_per_item(self) -> RowsPerItem {
        self.rows_per_item
    }

    #[must_use]
    pub const fn page_item_count(self, pane_rows: PaneRows) -> PageItemCount {
        PageItemCount::new(self.item_capacity(pane_rows).get())
    }
}

/// Horizontal content width for a bordered list with one column of content
/// padding on each side.
#[must_use]
pub const fn bordered_padded_content_width(pane_cols: u16) -> u16 {
    pane_cols.saturating_sub(4)
}
/// Selected-item-follow projection for a uniformly-sized list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListViewport {
    first_visible_item: usize,
    visible_range: Range<usize>,
    item_capacity: ItemCapacity,
}

impl ListViewport {
    #[must_use]
    pub fn uniform(
        item_count: usize,
        selected_item: Option<usize>,
        content_rows: ContentRows,
        rows_per_item: RowsPerItem,
    ) -> Self {
        let capacity = ItemCapacity::new(content_rows.get() / rows_per_item.get());
        if item_count == 0 || capacity.get() == 0 {
            return Self {
                first_visible_item: 0,
                visible_range: 0..0,
                item_capacity: capacity,
            };
        }

        let selected = selected_item.unwrap_or(0).min(item_count - 1);
        let visible_count = capacity.get().min(item_count);
        let max_first = item_count - visible_count;
        let first_visible_item = selected
            .saturating_add(1)
            .saturating_sub(visible_count)
            .min(max_first);
        Self {
            first_visible_item,
            visible_range: first_visible_item..first_visible_item + visible_count,
            item_capacity: capacity,
        }
    }

    #[must_use]
    pub const fn first_visible_item(&self) -> usize {
        self.first_visible_item
    }

    #[must_use]
    pub fn visible_range(&self) -> Range<usize> {
        self.visible_range.clone()
    }

    #[must_use]
    pub const fn item_capacity(&self) -> ItemCapacity {
        self.item_capacity
    }

    #[must_use]
    pub const fn page_item_count(&self) -> PageItemCount {
        PageItemCount::new(self.item_capacity.get())
    }
}

/// Deterministic movement request for a logical list selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMove {
    Up,
    Down,
    PageUp(PageItemCount),
    PageDown(PageItemCount),
    Home,
    End,
}

/// Move a selection within list bounds without consulting terminal state.
#[must_use]
pub fn move_selection(
    selected: Option<usize>,
    item_count: usize,
    movement: ListMove,
) -> Option<usize> {
    if item_count == 0 {
        return None;
    }
    if matches!(
        movement,
        ListMove::PageUp(PageItemCount(0)) | ListMove::PageDown(PageItemCount(0))
    ) {
        return selected.map(|index| index.min(item_count - 1));
    }
    let last = item_count - 1;
    let Some(current) = selected.map(|index| index.min(last)) else {
        return Some(if movement == ListMove::End { last } else { 0 });
    };
    Some(match movement {
        ListMove::Up => current.saturating_sub(1),
        ListMove::Down => current.saturating_add(1).min(last),
        ListMove::PageUp(page) => current.saturating_sub(page.get()),
        ListMove::PageDown(page) => current.saturating_add(page.get()).min(last),
        ListMove::Home => 0,
        ListMove::End => last,
    })
}

/// Fit text to terminal display cells, truncating with a single-cell ellipsis.
///
/// Text that already fits is returned unchanged. A zero-width destination is
/// empty. Truncation occurs only at UTF-8 character boundaries and accounts for
/// full-width and zero-width Unicode characters.
#[must_use]
pub fn fit_text_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }

    let content_width = width.saturating_sub(1);
    let mut fitted = String::new();
    let mut used: usize = 0;
    for character in text.chars() {
        let mut encoded = [0; 4];
        let character_width = UnicodeWidthStr::width(character.encode_utf8(&mut encoded));
        if used.saturating_add(character_width) > content_width {
            break;
        }
        fitted.push(character);
        used = used.saturating_add(character_width);
    }
    fitted.push('…');
    fitted
}
