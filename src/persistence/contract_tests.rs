//! Contract tests for shared diagnostics, bounds, and SHA-256.

use super::diagnostic::{
    ARRAY_LIMIT, CfgCode, DIAGNOSTIC_LIMIT, Diagnostic, DiagnosticPath, EFFECT_LIMIT, FILE_LIMIT,
    MAP_LIMIT, NESTING_LIMIT, PATH_LIMIT, PROVENANCE_LIMIT, STRING_LIMIT, Severity, Span,
    validate_inclusive_limit,
};
use super::sha256::Sha256;

#[test]
fn sha256_matches_standard_known_answer_vectors() {
    assert_eq!(
        Sha256::digest(b"").to_string(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        Sha256::digest(b"abc").to_string(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn sha256_wire_format_is_strict_lowercase_hex() {
    let digest = Sha256::digest(b"abc");
    let Ok(encoded) = serde_json::to_string(&digest) else {
        panic!("SHA-256 must serialize");
    };
    assert_eq!(
        encoded,
        r#""ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad""#
    );
    assert!(
        serde_json::from_str::<Sha256>(
            r#""BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD""#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<Sha256>(r#""abc""#).is_err());
}

#[test]
fn diagnostics_sort_by_severity_path_span_and_code() {
    let mut diagnostics = [
        diagnostic(CfgCode::E003, Severity::Error, "/b", Some(Span::new(3, 4))),
        diagnostic(CfgCode::W004, Severity::Warning, "/a", None),
        diagnostic(CfgCode::E002, Severity::Error, "/a", Some(Span::new(4, 5))),
        diagnostic(CfgCode::E001, Severity::Error, "/a", Some(Span::new(4, 5))),
        diagnostic(CfgCode::E008, Severity::Error, "/a", None),
    ];
    diagnostics.sort();
    let codes: Vec<_> = diagnostics.iter().map(|item| item.code).collect();
    assert_eq!(
        codes,
        vec![
            CfgCode::E008,
            CfgCode::E001,
            CfgCode::E002,
            CfgCode::E003,
            CfgCode::W004
        ]
    );
}

#[test]
fn every_inclusive_bound_accepts_limit_and_rejects_limit_plus_one() {
    for limit in [
        FILE_LIMIT,
        NESTING_LIMIT,
        MAP_LIMIT,
        ARRAY_LIMIT,
        STRING_LIMIT,
        PATH_LIMIT,
        DIAGNOSTIC_LIMIT,
        PROVENANCE_LIMIT,
        EFFECT_LIMIT,
    ] {
        assert!(validate_inclusive_limit(limit, limit, DiagnosticPath::root()).is_ok());
        let error = validate_inclusive_limit(limit + 1, limit, DiagnosticPath::root());
        assert_eq!(
            error.map_err(|diagnostic| diagnostic.code),
            Err(CfgCode::E008)
        );
    }
}

fn diagnostic(code: CfgCode, severity: Severity, path: &str, span: Option<Span>) -> Diagnostic {
    Diagnostic::new(
        code,
        severity,
        DiagnosticPath::new(path),
        span,
        "correct the value",
    )
}
