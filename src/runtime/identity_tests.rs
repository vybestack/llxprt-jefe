//! Behavioral contracts for privacy-conscious runtime namespace identity.

use super::identity::{namespace_for_identity, unique_namespace_for_identity};

#[test]
fn namespace_is_deterministic_private_and_valid() {
    let raw_identity = b"S-1-5-21-private-user-material";
    let first = namespace_for_identity(raw_identity);
    let second = namespace_for_identity(raw_identity);

    assert_eq!(first, second);
    assert!(first.starts_with("jefe-"));
    assert!(
        first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    assert!(!first.contains("private"));
    assert!(!first.contains("S-1-5"));
}

#[test]
fn namespaces_separate_users_and_parallel_runs() {
    let first_user = namespace_for_identity(b"user-one");
    let second_user = namespace_for_identity(b"user-two");
    assert_ne!(first_user, second_user);

    let first_run = unique_namespace_for_identity(b"user-one");
    let second_run = unique_namespace_for_identity(b"user-one");
    assert_ne!(first_run, second_run);
    assert!(first_run.starts_with(&first_user));
    assert!(second_run.starts_with(&first_user));
}
