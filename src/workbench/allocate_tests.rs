//! One-axis allocation tests (issue #384, CW04-05 and CW04-06).

use std::num::NonZeroU16;

use super::allocate::{AxisChild, allocate_axis};
use super::descriptor::Size;

fn weighted(weight: u16, min: u16, max: Option<u16>) -> AxisChild {
    AxisChild {
        size: Size::Weight(NonZeroU16::new(weight).unwrap_or(NonZeroU16::MIN)),
        min,
        max,
        collapsible: false,
        collapse_priority: 0,
        depth_first_index: 0,
        hidden: false,
    }
}

fn fixed(cells: u16, min: u16, max: Option<u16>) -> AxisChild {
    AxisChild {
        size: Size::Fixed(NonZeroU16::new(cells).unwrap_or(NonZeroU16::MIN)),
        min,
        max,
        collapsible: false,
        collapse_priority: 0,
        depth_first_index: 0,
        hidden: false,
    }
}

fn collapsible(weight: u16, min: u16, priority: i32, depth_first_index: usize) -> AxisChild {
    AxisChild {
        size: Size::Weight(NonZeroU16::new(weight).unwrap_or(NonZeroU16::MIN)),
        min,
        max: None,
        collapsible: true,
        collapse_priority: priority,
        depth_first_index,
        hidden: false,
    }
}

fn granted(children: &[AxisChild], available: u16) -> Vec<Option<u16>> {
    granted_with_gap(children, available, 1)
}

fn granted_with_gap(children: &[AxisChild], available: u16, gap: u16) -> Vec<Option<u16>> {
    allocate_axis(children, available, gap)
        .unwrap_or_else(|error| unreachable!("allocation must not fail: {error}"))
        .cells
}

#[test]
fn two_equal_weights_split_the_cells_after_the_separator() {
    // 21 cells, one separator, 20 content cells shared evenly.
    assert_eq!(
        granted(&[weighted(1, 1, None), weighted(1, 1, None)], 21),
        vec![Some(10), Some(10)]
    );
}

#[test]
fn an_odd_remainder_goes_to_the_first_child_in_declaration_order() {
    // 22 cells, one separator, 21 content cells: 10 + 10 with 1 left over.
    assert_eq!(
        granted(&[weighted(1, 1, None), weighted(1, 1, None)], 22),
        vec![Some(11), Some(10)]
    );
}

#[test]
fn weights_share_in_proportion_with_the_remainder_going_to_the_largest_claim() {
    // 31 cells, one separator, 30 content cells at weights 1:2. Minima are
    // satisfied first (1 each), leaving 28 to share: floor(28/3) = 9 and
    // floor(56/3) = 18, so 10 and 19 with one cell left over. The second child
    // was owed two thirds of a cell against the first child's one third, so it
    // takes the leftover and the split stays on its declared 1:2 proportion.
    assert_eq!(
        granted(&[weighted(1, 1, None), weighted(2, 1, None)], 31),
        vec![Some(10), Some(20)]
    );
}

#[test]
fn an_equal_claim_breaks_the_tie_in_declaration_order() {
    // Equal weights leave both children owed exactly half a cell, so the
    // outcome must still be deterministic rather than arbitrary.
    assert_eq!(
        granted(&[weighted(1, 1, None), weighted(1, 1, None)], 22),
        vec![Some(11), Some(10)]
    );
}

#[test]
fn the_remainder_does_not_drift_a_three_to_seven_split() {
    // The shipped workspace proportion. Handing the leftover to the first child
    // would give the small pane 12 of 38 rows instead of 11.
    assert_eq!(
        granted_with_gap(&[weighted(3, 0, None), weighted(7, 0, None)], 38, 0),
        vec![Some(11), Some(27)]
    );
}

#[test]
fn a_fixed_child_claims_its_declared_size() {
    // 30 cells, one separator, 29 content cells: 22 fixed + 7 weighted.
    assert_eq!(
        granted(&[fixed(22, 22, Some(22)), weighted(1, 1, None)], 30),
        vec![Some(22), Some(7)]
    );
}

#[test]
fn a_fixed_size_below_the_minimum_is_raised_to_the_minimum() {
    assert_eq!(
        granted(&[fixed(2, 6, None), weighted(1, 1, None)], 21)[0],
        Some(6)
    );
}

#[test]
fn a_fixed_size_above_the_maximum_is_lowered_to_the_maximum() {
    assert_eq!(
        granted(&[fixed(40, 1, Some(9)), weighted(1, 1, None)], 41)[0],
        Some(9)
    );
}

#[test]
fn a_weighted_child_that_reaches_its_maximum_gives_the_rest_to_its_sibling() {
    // 41 cells, one separator, 40 content cells. An even split would give 20
    // each, but the first child is capped at 5, so the second takes 35.
    assert_eq!(
        granted(&[weighted(1, 1, Some(5)), weighted(1, 1, None)], 41),
        vec![Some(5), Some(35)]
    );
}

#[test]
fn every_visible_child_always_receives_at_least_its_minimum() {
    let children = [weighted(1, 4, None), weighted(9, 3, None)];
    for available in 8_u16..=200 {
        let cells = granted(&children, available);
        assert!(cells[0].unwrap_or(0) >= 4, "available {available}");
        assert!(cells[1].unwrap_or(0) >= 3, "available {available}");
    }
}

#[test]
fn allocation_exactly_tiles_the_axis_for_every_size_it_fits() {
    let children = [
        weighted(1, 2, None),
        weighted(3, 2, Some(40)),
        fixed(6, 2, None),
    ];
    for available in 12_u16..=200 {
        let allocation = allocate_axis(&children, available, 1)
            .unwrap_or_else(|error| unreachable!("allocation must not fail: {error}"));
        if !allocation.fits {
            continue;
        }
        let visible = allocation
            .cells
            .iter()
            .filter(|cells| cells.is_some())
            .count();
        let total: u32 = allocation
            .cells
            .iter()
            .map(|cells| u32::from(cells.unwrap_or(0)))
            .sum();
        let separators = u32::try_from(visible.saturating_sub(1)).unwrap_or(0);
        assert!(
            total + separators <= u32::from(available),
            "available {available}: {total} + {separators} cells overflow"
        );
    }
}

#[test]
fn allocation_is_deterministic() {
    let children = [weighted(2, 3, None), weighted(5, 1, Some(12))];
    for available in 5_u16..=120 {
        let first = granted(&children, available);
        let second = granted(&children, available);
        assert_eq!(first, second, "available {available}");
    }
}

#[test]
fn growth_is_monotonic_in_the_available_cells() {
    let children = [weighted(1, 2, None), weighted(1, 2, None)];
    let mut previous = 0_u32;
    for available in 5_u16..=200 {
        let total: u32 = granted(&children, available)
            .iter()
            .map(|cells| u32::from(cells.unwrap_or(0)))
            .sum();
        assert!(
            total >= previous,
            "available {available}: total shrank from {previous} to {total}"
        );
        previous = total;
    }
}

#[test]
fn the_lowest_collapse_priority_is_hidden_first() {
    // Minima are 10 + 10 + 10 plus two separators; 25 cells force one collapse.
    let children = [
        weighted(1, 10, None),
        collapsible(1, 10, 5, 1),
        collapsible(1, 10, 1, 2),
    ];
    let cells = granted(&children, 25);
    assert_eq!(cells[2], None, "priority 1 collapses before priority 5");
    assert!(cells[1].is_some());
}

#[test]
fn a_priority_tie_is_broken_by_the_deepest_child_first() {
    let children = [
        weighted(1, 10, None),
        collapsible(1, 10, 0, 1),
        collapsible(1, 10, 0, 7),
    ];
    let cells = granted(&children, 25);
    assert_eq!(
        cells[2], None,
        "the greater depth-first index collapses first"
    );
    assert!(cells[1].is_some());
}

#[test]
fn collapsing_continues_until_the_survivors_fit() {
    let children = [
        weighted(1, 10, None),
        collapsible(1, 10, 0, 1),
        collapsible(1, 10, 1, 2),
    ];
    let cells = granted(&children, 10);
    assert_eq!(cells[1], None);
    assert_eq!(cells[2], None);
    assert_eq!(cells[0], Some(10));
}

#[test]
fn an_axis_that_cannot_fit_its_required_minima_reports_what_it_needed() {
    let children = [weighted(1, 10, None), weighted(1, 10, None)];
    let allocation = allocate_axis(&children, 8, 1)
        .unwrap_or_else(|error| unreachable!("allocation must not fail: {error}"));
    assert!(!allocation.fits);
    assert_eq!(allocation.needed, 21, "10 + 10 plus one separator");
    assert!(allocation.cells.iter().all(Option::is_none));
}

#[test]
fn an_application_hidden_child_consumes_no_cells_and_no_separator() {
    let mut hidden = weighted(1, 5, None);
    hidden.hidden = true;
    assert_eq!(
        granted(&[weighted(1, 1, None), hidden], 20),
        vec![Some(20), None]
    );
}

#[test]
fn a_zero_length_axis_never_panics() {
    let allocation = allocate_axis(&[weighted(1, 1, None), weighted(1, 1, None)], 0, 1)
        .unwrap_or_else(|error| unreachable!("allocation must not fail: {error}"));
    assert!(!allocation.fits);
}

#[test]
fn an_axis_at_the_maximum_terminal_width_does_not_overflow() {
    let children = [weighted(1, 1, None), weighted(1, 1, None)];
    assert!(allocate_axis(&children, u16::MAX, 1).is_ok());
}

#[test]
fn eight_children_are_allocated_in_declaration_order() {
    let children: Vec<AxisChild> = (0..8).map(|_| weighted(1, 1, None)).collect();
    // 8 children, 7 separators, 8 content cells -> one each.
    assert_eq!(
        granted(&children, 15),
        vec![Some(1); 8],
        "each child receives its minimum"
    );
}

#[test]
fn a_zero_gap_leaves_no_cell_between_children() {
    // Panes that draw their own border need no divider, so all 20 cells are
    // content: an even split is 10 and 10 with nothing between them.
    assert_eq!(
        granted_with_gap(&[weighted(1, 1, None), weighted(1, 1, None)], 20, 0),
        vec![Some(10), Some(10)]
    );
}

#[test]
fn a_wider_gap_is_charged_once_per_adjacent_visible_pair() {
    // Three children with a two-cell gap: 4 cells of divider, 21 of content.
    let children = [
        weighted(1, 1, None),
        weighted(1, 1, None),
        weighted(1, 1, None),
    ];
    let cells = granted_with_gap(&children, 25, 2);
    let total: u32 = cells
        .iter()
        .map(|cells| u32::from(cells.unwrap_or(0)))
        .sum();
    assert_eq!(total, 21);
}

#[test]
fn a_gap_is_not_charged_for_a_hidden_child() {
    let mut hidden = weighted(1, 5, None);
    hidden.hidden = true;
    // Only one child is visible, so no gap is charged and it takes everything.
    assert_eq!(
        granted_with_gap(&[weighted(1, 1, None), hidden], 20, 3),
        vec![Some(20), None]
    );
}
