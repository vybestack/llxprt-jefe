//! Process identity label tests moved out of the lib target (issue #307).

use jefe::{GIT_COMMIT, process_identity_label};

#[test]
fn label_formats_pid_and_commit() {
    let label = process_identity_label(12_345, "abc1234");
    assert_eq!(label, "pid:12345 abc1234");
}

#[test]
fn label_includes_pid_marker() {
    let label = process_identity_label(1, "deadbeef");
    assert!(
        label.starts_with("pid:1 "),
        "label must start with the pid marker: {label}"
    );
}

#[test]
fn label_includes_commit() {
    let label = process_identity_label(42, "feat0ab");
    assert!(
        label.ends_with(" feat0ab"),
        "label must end with the commit hash: {label}"
    );
}

#[test]
fn git_commit_is_non_empty() {
    assert!(!GIT_COMMIT.is_empty());
}
