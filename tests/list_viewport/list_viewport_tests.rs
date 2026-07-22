use jefe::list_viewport::{
    BorderRows, ListGeometry, ListMove, ListViewport, PaddingRows, PageItemCount, PaneRows,
    RowsPerItem, TitleRows, fit_text_to_width, move_selection,
};
use jefe::list_viewport::ContentRows;
use unicode_width::UnicodeWidthStr;

#[test]
fn uniform_viewport_table_covers_zero_rows_and_selection_following() {
    let cases = [
        (0, 0, 10, 1, 0, 0..0),
        (10, 4, 0, 1, 0, 0..0),
        (3, 1, 8, 2, 0, 0..3),
        (12, 0, 5, 1, 0, 0..5),
        (12, 8, 5, 1, 4, 4..9),
        (12, 11, 5, 1, 7, 7..12),
        (12, 8, 6, 2, 6, 6..9),
    ];

    for (len, selected, content_rows, extent, first, range) in cases {
        let viewport = ListViewport::uniform(
            len,
            Some(selected),
            ContentRows::new(content_rows),
            RowsPerItem::new(extent),
        );
        assert_eq!(
            viewport.first_visible_item(),
            first,
            "case len={len} selected={selected}"
        );
        assert_eq!(
            viewport.visible_range(),
            range,
            "case len={len} selected={selected}"
        );
    }
}

#[test]
fn viewport_invariants_hold_for_many_uniform_inputs() {
    for len in 0..40 {
        for selected in 0..45 {
            for rows in 0..16 {
                for extent in 1..=3 {
                    let viewport = ListViewport::uniform(
                        len,
                        Some(selected),
                        ContentRows::new(rows),
                        RowsPerItem::new(extent),
                    );
                    let range = viewport.visible_range();
                    assert!(range.start <= range.end);
                    assert!(range.end <= len);
                    assert!(range.len() <= viewport.item_capacity().get());
                    if len > 0 && viewport.item_capacity().get() > 0 {
                        let clamped = selected.min(len - 1);
                        assert!(range.contains(&clamped));
                    }
                    assert_eq!(
                        viewport.page_item_count().get(),
                        viewport.item_capacity().get()
                    );
                }
            }
        }
    }
}

#[test]
fn geometry_explicitly_subtracts_border_title_and_padding() {
    let geometry = ListGeometry::new(
        BorderRows::new(2),
        TitleRows::new(1),
        PaddingRows::new(2),
        RowsPerItem::new(2),
    );
    assert_eq!(geometry.content_rows(PaneRows::new(13)).get(), 8);
    assert_eq!(geometry.item_capacity(PaneRows::new(13)).get(), 4);
    assert_eq!(geometry.content_rows(PaneRows::new(4)).get(), 0);
}

#[test]
fn navigation_uses_typed_actual_page_capacity() {
    assert_eq!(
        move_selection(Some(10), 30, ListMove::PageUp(PageItemCount::new(3))),
        Some(7)
    );
    assert_eq!(
        move_selection(Some(10), 30, ListMove::PageDown(PageItemCount::new(7))),
        Some(17)
    );
    assert_eq!(
        move_selection(Some(10), 12, ListMove::PageDown(PageItemCount::new(7))),
        Some(11)
    );
    assert_eq!(
        move_selection(Some(10), 30, ListMove::PageDown(PageItemCount::new(0))),
        Some(10)
    );
    assert_eq!(
        move_selection(None, 30, ListMove::PageDown(PageItemCount::new(7))),
        Some(0)
    );
    assert_eq!(move_selection(None, 30, ListMove::End), Some(29));
    assert_eq!(move_selection(None, 0, ListMove::End), None);
}

#[test]
fn unicode_fitting_never_exceeds_width_and_keeps_exact_text_when_it_fits() {
    let samples = [
        "plain",
        "１２wide",
        "e\u{0301}combining",
        "repository/very-long-name",
    ];
    for sample in samples {
        for width in 0..16 {
            let fitted = fit_text_to_width(sample, width);
            assert!(UnicodeWidthStr::width(fitted.as_str()) <= width);
            if UnicodeWidthStr::width(sample) <= width {
                assert_eq!(fitted, sample);
            }
        }
    }
    assert_eq!(fit_text_to_width("abcdef", 4), "abc…");
    assert_eq!(fit_text_to_width("１２abc", 4), "１…");
}
