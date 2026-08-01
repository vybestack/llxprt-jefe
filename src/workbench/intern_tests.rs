//! Behavior of the bounded identifier interner (issue #385).

use super::intern::{MAX_INTERNED_IDENTIFIERS, intern, resident_count};

#[test]
fn interning_the_same_text_twice_yields_one_shared_allocation() {
    let first = intern("issue-list")
        .unwrap_or_else(|error| unreachable!("first intern must succeed: {error}"));
    let second = intern("issue-list")
        .unwrap_or_else(|error| unreachable!("second intern must succeed: {error}"));

    assert_eq!(first, second);
    assert!(
        std::ptr::eq(first, second),
        "repeated interning must reuse one allocation, not leak a second copy"
    );
}

#[test]
fn interning_distinct_text_yields_distinct_values() {
    let list = intern("intern-distinct-list")
        .unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));
    let detail = intern("intern-distinct-detail")
        .unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));

    assert_ne!(list, detail);
}

#[test]
fn interned_text_is_byte_identical_to_its_input() {
    let value = "local.review-42";

    let interned =
        intern(value).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));

    assert_eq!(interned, value);
}

#[test]
fn interning_admits_text_the_declared_limits_can_produce() {
    // 64 screens * (identity + route + 16 panels * (identity + type + 32 ports
    // * (identity + versioned type))) is the worst legal directory, so the
    // ceiling must not sit below it.
    assert_eq!(MAX_INTERNED_IDENTIFIERS, 64 * (2 + 16 * (2 + 32 * 2)));
}

#[test]
fn interning_empty_text_is_permitted_and_stable() {
    // Grammar rejection happens before interning; the table itself only has to
    // be total, so an empty string must not be a special case here.
    let first = intern("").unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));
    let second = intern("").unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));

    assert!(std::ptr::eq(first, second));
}

#[test]
fn resident_count_grows_by_one_for_new_text_and_not_at_all_for_repeated_text() {
    let unique = "intern-resident-count-probe";
    let before = resident_count();

    intern(unique).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));
    let after_first = resident_count();
    intern(unique).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));

    assert_eq!(after_first, before + 1, "new text admits exactly one entry");
    assert_eq!(resident_count(), after_first, "repeated text admits none");
}
