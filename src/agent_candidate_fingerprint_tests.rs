//! Unit tests for [`CandidateFingerprint`].

use super::{CandidateFingerprint, FingerprintCaptureError, capture_candidate_fingerprint};

#[test]
fn new_captures_all_fields() {
    let fp = CandidateFingerprint::new(
        std::path::PathBuf::from("/bin/llxprt"),
        Some(42),
        Some(99),
        1_024,
        1_700_000_000,
    );
    assert_eq!(fp.canonical_path(), std::path::Path::new("/bin/llxprt"));
    assert_eq!(fp.dev(), Some(42));
    assert_eq!(fp.ino(), Some(99));
    assert_eq!(fp.size(), 1_024);
    assert_eq!(fp.mtime_secs(), 1_700_000_000);
    assert!(fp.has_dev_ino());
}

#[test]
fn fingerprint_without_dev_ino_reports_absent() {
    let fp = CandidateFingerprint::new(
        std::path::PathBuf::from("C:/bin/llxprt.exe"),
        None,
        None,
        8,
        10,
    );
    assert!(!fp.has_dev_ino());
}

#[test]
fn fingerprints_eq_when_fields_match() {
    let a = CandidateFingerprint::new(
        std::path::PathBuf::from("/bin/llxprt"),
        Some(1),
        Some(2),
        3,
        4,
    );
    let b = a.clone();
    assert_eq!(a, b);
    let different_size = CandidateFingerprint::new(
        std::path::PathBuf::from("/bin/llxprt"),
        Some(1),
        Some(2),
        999,
        4,
    );
    assert_ne!(a, different_size);
}

#[test]
fn display_includes_path_size_mtime_and_dev_ino_when_present() {
    let fp = CandidateFingerprint::new(
        std::path::PathBuf::from("/bin/llxprt"),
        Some(7),
        Some(8),
        3,
        4,
    );
    let s = fp.to_string();
    assert!(s.contains("/bin/llxprt"), "{s}");
    assert!(s.contains("size=3"), "{s}");
    assert!(s.contains("mtime=4"), "{s}");
    assert!(s.contains("dev=7"), "{s}");
    assert!(s.contains("ino=8"), "{s}");
}

#[test]
fn timestamp_parts_preserve_pre_epoch_sign_and_subseconds() {
    let before = std::time::UNIX_EPOCH - std::time::Duration::from_millis(250);
    let after = std::time::UNIX_EPOCH + std::time::Duration::from_millis(250);

    assert_eq!(super::timestamp_parts(before), (-1, 750_000_000));
    assert_eq!(super::timestamp_parts(after), (0, 250_000_000));
}

#[test]
fn display_includes_subsecond_modification_time() {
    let fp = CandidateFingerprint::with_mtime_nanos(
        std::path::PathBuf::from("/bin/llxprt"),
        Some(7),
        Some(8),
        3,
        4,
        123_456_789,
    );

    assert!(fp.to_string().contains("mtime=4.123456789"));
}

#[test]
fn missing_executable_reports_typed_canonicalize_error() {
    let missing =
        std::env::temp_dir().join(format!("jefe-missing-fingerprint-{}", std::process::id()));

    let error = capture_candidate_fingerprint(&missing)
        .err()
        .unwrap_or_else(|| panic!("missing executable must fail fingerprint capture"));
    match error {
        FingerprintCaptureError::Canonicalize { path, source } => {
            assert_eq!(path, missing);
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        }
        other => panic!("expected canonicalize error, got {other}"),
    }
}
