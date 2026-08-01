//! Behavioral tests for the Windows per-module coverage gate (#545, V5).
//!
//! V5 requires that the gate "fails when a Windows-only module's coverage
//! drops below its floor", demonstrated by a deliberate regression. These
//! tests drive that regression through the evaluator with synthetic LCOV, so
//! the failure is proven deterministically rather than by waiting for a real
//! coverage drop.

use std::fmt::Write as _;

use xtask::windows_coverage::{
    FloorViolation, ModuleFloor, WINDOWS_MODULE_FLOORS, evaluate_floors, parse_lcov, render_report,
};

/// Build an LCOV tracefile for the given (path, found, hit) triples.
fn lcov(records: &[(&str, u64, u64)]) -> String {
    let mut text = String::new();
    for (path, found, hit) in records {
        let _ = writeln!(text, "TN:\nSF:{path}\nLF:{found}\nLH:{hit}\nend_of_record");
    }
    text
}

#[test]
fn parses_line_totals_per_source_file() {
    let parsed = parse_lcov(&lcov(&[
        ("src/runtime/attach.rs", 200, 150),
        ("src/runtime/job_object.rs", 50, 25),
    ]));
    assert_eq!(parsed.len(), 2);
    let attach = parsed
        .iter()
        .find(|file| file.path == "src/runtime/attach.rs")
        .unwrap_or_else(|| panic!("attach.rs must be parsed"));
    assert_eq!(attach.lines_found, 200);
    assert_eq!(attach.lines_hit, 150);
    assert_eq!(attach.basis_points(), 7_500);
    assert!(attach.meets_floor(75));
    assert!(!attach.meets_floor(76));
}

#[test]
fn sums_repeated_records_for_the_same_file() {
    // llvm-cov emits one record per test binary; the gate must aggregate them
    // rather than let the last record win.
    let parsed = parse_lcov(&lcov(&[
        ("src/runtime/attach.rs", 100, 40),
        ("src/runtime/attach.rs", 100, 30),
    ]));
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].lines_found, 200);
    assert_eq!(parsed[0].lines_hit, 70);
}

#[test]
fn normalizes_windows_separators_and_absolute_paths() {
    let parsed =
        parse_lcov("SF:C:\\ci\\jefe\\src\\runtime\\attach.rs\nLF:10\nLH:5\nend_of_record\n");
    assert_eq!(parsed[0].path, "C:/ci/jefe/src/runtime/attach.rs");
    let floors = [ModuleFloor {
        path: "src/runtime/attach.rs",
        floor_percent: 50,
    }];
    // An absolute path from the coverage tool must still satisfy a
    // repository-relative floor.
    assert!(evaluate_floors(&parsed, &floors).is_empty());
}

/// V5: the deliberate regression. A module below its floor fails the gate and
/// is named with both the measured value and the floor.
#[test]
fn module_below_its_floor_is_reported_as_a_violation() {
    let floors = [ModuleFloor {
        path: "src/runtime/job_object.rs",
        floor_percent: 60,
    }];
    let coverage = parse_lcov(&lcov(&[("src/runtime/job_object.rs", 100, 59)]));
    let violations = evaluate_floors(&coverage, &floors);
    assert_eq!(violations.len(), 1, "a regression must fail the gate");
    match &violations[0] {
        FloorViolation::Below {
            path,
            actual_basis_points,
            floor_percent,
        } => {
            assert_eq!(path, "src/runtime/job_object.rs");
            assert_eq!(*actual_basis_points, 5_900);
            assert_eq!(*floor_percent, 60);
        }
        other @ FloorViolation::Missing { .. } => {
            panic!("expected a Below violation, got {other:?}")
        }
    }
    let rendered = violations[0].to_string();
    assert!(rendered.contains("src/runtime/job_object.rs"), "{rendered}");
    assert!(rendered.contains("59.00%"), "{rendered}");
    assert!(rendered.contains("60%"), "{rendered}");
}

#[test]
fn module_exactly_at_its_floor_passes() {
    let floors = [ModuleFloor {
        path: "src/runtime/job_object.rs",
        floor_percent: 60,
    }];
    let coverage = parse_lcov(&lcov(&[("src/runtime/job_object.rs", 100, 60)]));
    assert!(evaluate_floors(&coverage, &floors).is_empty());
}

/// A module that vanishes from the report must fail, otherwise renaming a file
/// or accidentally compiling it out would silently retire its floor.
#[test]
fn module_absent_from_the_report_is_a_violation() {
    let floors = [ModuleFloor {
        path: "src/runtime/job_object.rs",
        floor_percent: 10,
    }];
    let coverage = parse_lcov(&lcov(&[("src/runtime/attach.rs", 100, 100)]));
    let violations = evaluate_floors(&coverage, &floors);
    assert_eq!(violations.len(), 1);
    assert!(
        matches!(&violations[0], FloorViolation::Missing { path } if path == "src/runtime/job_object.rs"),
        "expected a Missing violation, got {:?}",
        violations[0]
    );
}

/// The shipped floor table must name exactly the Windows-only modules this
/// epic changes, so the gate cannot quietly stop watching one of them.
#[test]
fn shipped_floors_cover_every_windows_only_module_named_by_the_issue() {
    let paths: Vec<&str> = WINDOWS_MODULE_FLOORS
        .iter()
        .map(|floor| floor.path)
        .collect();
    for required in [
        "src/runtime/job_object.rs",
        "src/app_shell_liveness.rs",
        "src/runtime/server_health_io.rs",
        "src/runtime/session_host.rs",
        "src/runtime/agent_launcher.rs",
        "src/runtime/attach.rs",
    ] {
        assert!(
            paths.contains(&required),
            "{required} must have a Windows coverage floor"
        );
    }
}

#[test]
fn report_lists_every_configured_module() {
    let coverage = parse_lcov(&lcov(&[("src/runtime/attach.rs", 100, 42)]));
    let report = render_report(&coverage, WINDOWS_MODULE_FLOORS);
    for floor in WINDOWS_MODULE_FLOORS {
        assert!(
            report.contains(floor.path),
            "report must list {}",
            floor.path
        );
    }
    assert!(
        report.contains("42.00%"),
        "report must show measured values"
    );
    assert!(report.contains("ABSENT"), "report must flag absent modules");
}
