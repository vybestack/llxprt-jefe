use jefe::domain::{CLOSE_REASONS, CloseReason};

#[test]
fn completed_label_and_flag() {
    assert_eq!(CloseReason::Completed.label(), "Completed");
    assert_eq!(CloseReason::Completed.gh_reason_flag(), "completed");
}

#[test]
fn not_planned_label_and_flag() {
    assert_eq!(CloseReason::NotPlanned.label(), "Not planned");
    assert_eq!(CloseReason::NotPlanned.gh_reason_flag(), "not planned");
}

#[test]
fn duplicate_label_and_flag() {
    assert_eq!(CloseReason::Duplicate.label(), "Duplicate");
    assert_eq!(CloseReason::Duplicate.gh_reason_flag(), "not planned");
}

#[test]
fn invalid_label_and_flag() {
    assert_eq!(CloseReason::Invalid.label(), "Invalid");
    assert_eq!(CloseReason::Invalid.gh_reason_flag(), "not planned");
}

#[test]
fn close_reasons_has_four_variants_in_order() {
    assert_eq!(
        CLOSE_REASONS,
        [
            CloseReason::Completed,
            CloseReason::NotPlanned,
            CloseReason::Duplicate,
            CloseReason::Invalid,
        ]
    );
}
