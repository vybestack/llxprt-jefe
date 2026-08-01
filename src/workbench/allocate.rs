//! Deterministic one-axis cell allocation (issue #384, CW04-05 and CW04-06).
//!
//! This is the whole of the sizing algorithm, isolated from the layout tree so
//! it can be swept exhaustively as pure arithmetic. The rules, in order:
//!
//! 1. The split's declared gap is reserved between each adjacent pair of
//!    *visible* children. A pane that draws its own border inside its own
//!    rectangle needs no gap and declares zero; a gap is for splits that want a
//!    drawn or blank divider between children.
//! 2. While the visible children's minima do not fit, hide one collapsible
//!    child chosen by `(collapse_priority ascending, depth_first_index
//!    descending)`. If the survivors still do not fit, the axis does not fit
//!    and the caller falls back to the too-small layout.
//! 3. Fixed children claim their declared size clamped to `[min, max]`.
//! 4. Weighted children start at their minimum, then share the cells left over
//!    in proportion to their weights: `floor(remaining * weight / sum_weight)`.
//! 5. A child that would exceed its maximum is pinned there, removed from the
//!    pool, and the distribution repeats with the cells it gave back.
//! 6. Any remainder is handed out one cell at a time in declaration order.
//!
//! All interior arithmetic is checked `u32`; overflow is a typed error.

use super::descriptor::Size;

/// One child of a split, reduced to the values allocation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisChild {
    /// How the child claims cells.
    pub size: Size,
    /// Fewest cells the child may occupy while visible.
    pub min: u16,
    /// Most cells the child may occupy, if bounded.
    pub max: Option<u16>,
    /// Whether the child may be hidden to fit its siblings.
    pub collapsible: bool,
    /// Collapse order key; lower values are hidden first.
    pub collapse_priority: i32,
    /// Depth-first index of the child, used to break collapse-priority ties.
    pub depth_first_index: usize,
    /// Whether the application has already hidden this child.
    pub hidden: bool,
}

/// Why an axis could not be allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    /// Interior arithmetic exceeded the checked range.
    Overflow,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => formatter.write_str("layout arithmetic overflowed"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Result of allocating one axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisAllocation {
    /// Cells granted to each child, in declaration order. A hidden child
    /// receives `None`.
    pub cells: Vec<Option<u16>>,
    /// Whether every child that had to stay visible fit.
    pub fits: bool,
    /// Cells the surviving children needed when they did not fit.
    pub needed: u32,
}

/// Allocate `available` cells among `children` along one axis.
///
/// # Errors
///
/// Returns [`LayoutError::Overflow`] if interior arithmetic leaves the checked
/// range. The function never panics and never wraps.
pub fn allocate_axis(
    children: &[AxisChild],
    available: u16,
    gap: u16,
) -> Result<AxisAllocation, LayoutError> {
    let mut visible: Vec<bool> = children.iter().map(|child| !child.hidden).collect();
    let available = u32::from(available);

    let needed = loop {
        let required = minimum_span(children, &visible, gap)?;
        if required <= available {
            break required;
        }
        let Some(victim) = next_collapse_victim(children, &visible) else {
            break required;
        };
        visible[victim] = false;
    };

    if needed > available {
        return Ok(AxisAllocation {
            cells: children.iter().map(|_| None).collect(),
            fits: false,
            needed,
        });
    }

    let separators = separator_cells(&visible, gap)?;
    let content = available
        .checked_sub(separators)
        .ok_or(LayoutError::Overflow)?;
    let cells = distribute(children, &visible, content)?;
    Ok(AxisAllocation {
        cells,
        fits: true,
        needed,
    })
}

/// Cells the currently visible children need at their minima, plus separators.
fn minimum_span(children: &[AxisChild], visible: &[bool], gap: u16) -> Result<u32, LayoutError> {
    let mut total = separator_cells(visible, gap)?;
    for (child, shown) in children.iter().zip(visible) {
        if *shown {
            total = total
                .checked_add(u32::from(child.min))
                .ok_or(LayoutError::Overflow)?;
        }
    }
    Ok(total)
}

/// The declared gap, charged once per adjacent visible pair.
fn separator_cells(visible: &[bool], gap: u16) -> Result<u32, LayoutError> {
    let shown = u32::try_from(visible.iter().filter(|shown| **shown).count())
        .map_err(|_| LayoutError::Overflow)?;
    shown
        .saturating_sub(1)
        .checked_mul(u32::from(gap))
        .ok_or(LayoutError::Overflow)
}

/// The next collapsible child to hide: lowest `collapse_priority`, then
/// greatest `depth_first_index`.
fn next_collapse_victim(children: &[AxisChild], visible: &[bool]) -> Option<usize> {
    children
        .iter()
        .enumerate()
        .filter(|(index, child)| child.collapsible && visible.get(*index).copied().unwrap_or(false))
        .min_by(|(_, left), (_, right)| {
            left.collapse_priority
                .cmp(&right.collapse_priority)
                .then(right.depth_first_index.cmp(&left.depth_first_index))
        })
        .map(|(index, _)| index)
}

/// Assign `content` cells to the visible children.
fn distribute(
    children: &[AxisChild],
    visible: &[bool],
    content: u32,
) -> Result<Vec<Option<u16>>, LayoutError> {
    let mut granted: Vec<Option<u32>> = Vec::with_capacity(children.len());
    // Weighted children that may still grow, in declaration order.
    let mut pool: Vec<usize> = Vec::new();

    for (index, (child, shown)) in children.iter().zip(visible).enumerate() {
        if !shown {
            granted.push(None);
            continue;
        }
        match child.size {
            Size::Fixed(cells) => {
                granted.push(Some(clamp_to_bounds(u32::from(cells.get()), child)));
            }
            Size::Weight(_) => {
                granted.push(Some(u32::from(child.min)));
                // A child whose maximum is already met by its minimum cannot
                // grow, so it never joins the distribution pool.
                let already_pinned = child
                    .max
                    .is_some_and(|max| u32::from(max) <= u32::from(child.min));
                if !already_pinned {
                    pool.push(index);
                }
            }
        }
    }

    let assigned = granted
        .iter()
        .try_fold(0_u32, |total, cells| total.checked_add(cells.unwrap_or(0)))
        .ok_or(LayoutError::Overflow)?;
    let mut remaining = content.saturating_sub(assigned);

    remaining = grow_by_weight(children, &mut granted, &mut pool, remaining)?;
    distribute_remainder(children, &mut granted, &pool, remaining);

    granted
        .into_iter()
        .map(|cells| match cells {
            None => Ok(None),
            Some(value) => u16::try_from(value)
                .map(Some)
                .map_err(|_| LayoutError::Overflow),
        })
        .collect()
}

/// Share `remaining` cells by weight, pinning any child that reaches its
/// maximum and redistributing the cells it gave back.
fn grow_by_weight(
    children: &[AxisChild],
    granted: &mut [Option<u32>],
    pool: &mut Vec<usize>,
    mut remaining: u32,
) -> Result<u32, LayoutError> {
    while remaining > 0 && !pool.is_empty() {
        let sum_weight = pool
            .iter()
            .try_fold(0_u32, |total, index| {
                total.checked_add(weight_of(children.get(*index)))
            })
            .ok_or(LayoutError::Overflow)?;
        if sum_weight == 0 {
            break;
        }

        let mut pinned_any = false;
        let mut consumed = 0_u32;
        for index in pool.clone() {
            let share = remaining
                .checked_mul(weight_of(children.get(index)))
                .ok_or(LayoutError::Overflow)?
                / sum_weight;
            if share == 0 {
                continue;
            }
            let Some(child) = children.get(index) else {
                continue;
            };
            let Some(Some(current)) = granted.get(index).copied() else {
                continue;
            };
            let target = current.checked_add(share).ok_or(LayoutError::Overflow)?;
            let capped = clamp_to_bounds(target, child);
            if let Some(slot) = granted.get_mut(index) {
                *slot = Some(capped);
            }
            consumed = consumed
                .checked_add(capped.saturating_sub(current))
                .ok_or(LayoutError::Overflow)?;
            if child.max.is_some_and(|max| capped >= u32::from(max)) {
                pool.retain(|candidate| *candidate != index);
                pinned_any = true;
            }
        }

        remaining = remaining.saturating_sub(consumed);
        if consumed == 0 && !pinned_any {
            break;
        }
    }
    Ok(remaining)
}

/// Hand out leftover cells one at a time in declaration order.
fn distribute_remainder(
    children: &[AxisChild],
    granted: &mut [Option<u32>],
    pool: &[usize],
    mut remaining: u32,
) {
    while remaining > 0 {
        let mut placed = false;
        for index in pool {
            if remaining == 0 {
                break;
            }
            let (Some(child), Some(Some(current))) =
                (children.get(*index), granted.get(*index).copied())
            else {
                continue;
            };
            if child.max.is_some_and(|max| current >= u32::from(max)) {
                continue;
            }
            if let Some(slot) = granted.get_mut(*index) {
                *slot = Some(current.saturating_add(1));
            }
            remaining -= 1;
            placed = true;
        }
        if !placed {
            break;
        }
    }
}

const fn weight_of(child: Option<&AxisChild>) -> u32 {
    match child {
        Some(AxisChild {
            size: Size::Weight(weight),
            ..
        }) => weight.get() as u32,
        _ => 0,
    }
}

/// Clamp a candidate size into the child's `[min, max]` bounds.
fn clamp_to_bounds(candidate: u32, child: &AxisChild) -> u32 {
    let lower = u32::from(child.min);
    let upper = child.max.map_or(u32::MAX, u32::from);
    candidate.clamp(lower.min(upper), upper.max(lower))
}
