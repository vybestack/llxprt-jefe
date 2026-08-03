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
fn interning_known_text_again_admits_no_new_entry() {
    // The table is process-global and tests run on several threads, so an exact
    // count delta would measure other tests. The local property is what matters:
    // known text reuses its entry, and the table only ever grows.
    let unique = "intern-resident-count-probe";
    let first = intern(unique).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));
    let before = resident_count();

    let second =
        intern(unique).unwrap_or_else(|error| unreachable!("intern must succeed: {error}"));

    assert!(
        std::ptr::eq(first, second),
        "known text must not admit a second entry"
    );
    assert!(
        resident_count() >= before,
        "the table never removes entries"
    );
}
