//! PageToken and ListRequestId tests moved out of the lib target (issue #307).

use jefe::domain::{ListRequestId, PageToken};

#[test]
fn cursor_with_more_yields_cursor() {
    let token = PageToken::from_cursor(Some("abc".to_string()), true);
    assert_eq!(token, PageToken::Cursor("abc".to_string()));
}

#[test]
fn cursor_without_more_yields_done() {
    let token = PageToken::from_cursor(Some("abc".to_string()), false);
    assert_eq!(token, PageToken::Done);
}

#[test]
fn missing_cursor_with_more_yields_done() {
    let token = PageToken::from_cursor(None, true);
    assert_eq!(token, PageToken::Done);
}

#[test]
fn rest_page_1_with_more_yields_page_2() {
    let token = PageToken::after_page(1, true);
    assert_eq!(token, PageToken::PageNumber(2));
}

#[test]
fn rest_page_without_more_yields_done() {
    let token = PageToken::after_page(3, false);
    assert_eq!(token, PageToken::Done);
}

#[test]
fn rest_page_at_u32_max_with_more_yields_done() {
    // Overflow must terminate pagination rather than wrapping the page number.
    let token = PageToken::after_page(u32::MAX, true);
    assert_eq!(token, PageToken::Done);
}

#[test]
fn has_more_true_only_for_non_done() {
    assert!(PageToken::Cursor("x".to_string()).has_more());
    assert!(PageToken::PageNumber(5).has_more());
    assert!(!PageToken::Done.has_more());
}

#[test]
fn list_request_id_default_is_zero() {
    assert_eq!(ListRequestId::default().get(), 0);
}

#[test]
fn list_request_id_default_matches_from_raw_zero() {
    assert_eq!(ListRequestId::default(), ListRequestId::from_raw(0));
}

#[test]
fn list_request_id_checked_next_from_zero() {
    let id = ListRequestId::from_raw(0);
    assert_eq!(id.checked_next(), Some(ListRequestId::from_raw(1)));
}

#[test]
fn list_request_id_checked_next_increments() {
    let id = ListRequestId::from_raw(41);
    assert_eq!(id.checked_next(), Some(ListRequestId::from_raw(42)));
}

#[test]
fn list_request_id_checked_next_at_max_returns_none() {
    let id = ListRequestId::from_raw(u64::MAX);
    assert_eq!(id.checked_next(), None);
}
