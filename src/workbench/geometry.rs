//! Rectangles and extents used by the layout resolver (issue #384).
//!
//! All interior arithmetic is checked `u32`: a layout can never silently wrap
//! or saturate its way into a wrong rectangle, and an out-of-range result is a
//! typed error rather than a panic.

use std::fmt;

/// A size in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Extent {
    /// Width in columns.
    pub cols: u16,
    /// Height in rows.
    pub rows: u16,
}

impl Extent {
    /// Build an extent.
    #[must_use]
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }

    /// Whether the extent encloses at least one cell.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.cols == 0 || self.rows == 0
    }
}

impl fmt::Display for Extent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x{}", self.cols, self.rows)
    }
}

/// A rectangle in zero-based render-grid coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    /// Zero-based column of the left edge.
    pub col: u16,
    /// Zero-based row of the top edge.
    pub row: u16,
    /// Width in columns.
    pub width: u16,
    /// Height in rows.
    pub height: u16,
}

impl Rect {
    /// Build a rectangle.
    #[must_use]
    pub const fn new(col: u16, row: u16, width: u16, height: u16) -> Self {
        Self {
            col,
            row,
            width,
            height,
        }
    }

    /// The rectangle's size.
    #[must_use]
    pub const fn extent(self) -> Extent {
        Extent::new(self.width, self.height)
    }

    /// Whether the rectangle encloses at least one cell.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Exclusive right edge.
    #[must_use]
    pub const fn right(self) -> u32 {
        self.col as u32 + self.width as u32
    }

    /// Exclusive bottom edge.
    #[must_use]
    pub const fn bottom(self) -> u32 {
        self.row as u32 + self.height as u32
    }

    /// Whether a cell lies inside the rectangle.
    #[must_use]
    pub const fn contains(self, col: u16, row: u16) -> bool {
        (col as u32) >= (self.col as u32)
            && (col as u32) < self.right()
            && (row as u32) >= (self.row as u32)
            && (row as u32) < self.bottom()
    }

    /// Whether two rectangles share any cell.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        !(self.is_empty()
            || other.is_empty()
            || self.right() <= other.col as u32
            || other.right() <= self.col as u32
            || self.bottom() <= other.row as u32
            || other.bottom() <= self.row as u32)
    }

    /// Shrink the rectangle by per-edge insets, clamping at zero.
    ///
    /// Used to derive a panel's content rectangle from the rectangle that
    /// includes its border and title, which the descriptor places *inside* the
    /// child's allocation.
    #[must_use]
    pub fn inset(self, insets: Insets) -> Self {
        let width = self
            .width
            .saturating_sub(insets.left)
            .saturating_sub(insets.right);
        let height = self
            .height
            .saturating_sub(insets.top)
            .saturating_sub(insets.bottom);
        if width == 0 || height == 0 {
            return Self::new(self.col, self.row, 0, 0);
        }
        Self::new(
            self.col.saturating_add(insets.left),
            self.row.saturating_add(insets.top),
            width,
            height,
        )
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}x{}+{}+{}",
            self.width, self.height, self.col, self.row
        )
    }
}

/// Per-edge chrome consumed inside a panel's rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Insets {
    /// Rows consumed at the top (border, title).
    pub top: u16,
    /// Rows consumed at the bottom (border).
    pub bottom: u16,
    /// Columns consumed on the left (border, padding).
    pub left: u16,
    /// Columns consumed on the right (border, padding).
    pub right: u16,
}

impl Insets {
    /// Build per-edge insets.
    #[must_use]
    pub const fn new(top: u16, bottom: u16, left: u16, right: u16) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    /// Total rows consumed vertically.
    #[must_use]
    pub const fn vertical(self) -> u32 {
        self.top as u32 + self.bottom as u32
    }

    /// Total columns consumed horizontally.
    #[must_use]
    pub const fn horizontal(self) -> u32 {
        self.left as u32 + self.right as u32
    }
}
