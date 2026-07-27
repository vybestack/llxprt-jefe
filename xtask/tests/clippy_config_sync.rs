//! Clippy.toml threshold sync tests (issue #459, A5).
//!
//! The five complexity thresholds in root `clippy.toml` and
//! `.github/clippy/clippy.toml` must be present and equal, so `CLIPPY_CONF_DIR`
//! cannot silently fall back to clippy defaults.

use std::fs;

use tempfile::TempDir;
use xtask::clippy_policy::check_config_sync;

const ROOT_CONFIG: &str = r#"msrv = "1.75"
cognitive-complexity-threshold = 15
too-many-lines-threshold = 60
too-many-arguments-threshold = 6
max-struct-bools = 3
type-complexity-threshold = 250
"#;

const CI_CONFIG: &str = r#"msrv = "1.75"
cognitive-complexity-threshold = 15
too-many-lines-threshold = 60
too-many-arguments-threshold = 6
max-struct-bools = 3
type-complexity-threshold = 250
"#;

/// Create a fixture repo root with the given root and CI clippy.toml text.
fn fixture(root_text: &str, ci_text: &str) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::write(dir.path().join("clippy.toml"), root_text).expect("root config");
    let ci_dir = dir.path().join(".github").join("clippy");
    fs::create_dir_all(&ci_dir).expect("ci config dir");
    fs::write(ci_dir.join("clippy.toml"), ci_text).expect("ci config");
    dir
}

#[test]
fn matched_configs_pass() {
    let dir = fixture(ROOT_CONFIG, CI_CONFIG);
    assert!(check_config_sync(dir.path()).is_ok());
}

#[test]
fn mismatched_threshold_fails() {
    let mut ci = CI_CONFIG.to_string();
    ci = ci.replace(
        "too-many-lines-threshold = 60",
        "too-many-lines-threshold = 99",
    );
    let dir = fixture(ROOT_CONFIG, &ci);
    let result = check_config_sync(dir.path());
    assert!(result.is_err(), "mismatched threshold must fail");
    let errors = result.unwrap_err().join("\n");
    assert!(
        errors.contains("mismatch for too-many-lines-threshold"),
        "error should name the mismatched key: {errors}"
    );
}

#[test]
fn missing_threshold_in_root_fails() {
    let root = r#"msrv = "1.75"
cognitive-complexity-threshold = 15
"#;
    let dir = fixture(root, CI_CONFIG);
    let result = check_config_sync(dir.path());
    assert!(result.is_err());
    let errors = result.unwrap_err().join("\n");
    assert!(
        errors.contains("too-many-lines-threshold"),
        "error should name the missing root key: {errors}"
    );
}

#[test]
fn missing_threshold_in_ci_fails() {
    let ci = r#"msrv = "1.75"
cognitive-complexity-threshold = 15
"#;
    let dir = fixture(ROOT_CONFIG, ci);
    let result = check_config_sync(dir.path());
    assert!(result.is_err());
    let errors = result.unwrap_err().join("\n");
    assert!(
        errors.contains(".github/clippy/clippy.toml is missing clippy threshold"),
        "error should name the missing CI key: {errors}"
    );
}

#[test]
fn missing_root_config_file_fails() {
    let dir = TempDir::new().expect("temp dir");
    let ci_dir = dir.path().join(".github").join("clippy");
    fs::create_dir_all(&ci_dir).expect("ci config dir");
    fs::write(ci_dir.join("clippy.toml"), CI_CONFIG).expect("ci config");
    let result = check_config_sync(dir.path());
    assert!(result.is_err());
    let errors = result.unwrap_err().join("\n");
    assert!(
        errors.contains("required file is missing"),
        "missing root config must be reported: {errors}"
    );
}

#[test]
fn trailing_comment_does_not_break_parsing() {
    let root = "cognitive-complexity-threshold = 15 # complexity\n";
    let ci = "cognitive-complexity-threshold = 15\n";
    let dir = fixture(root, ci);
    let result = check_config_sync(dir.path());
    // Only the cognitive-complexity key is present; the other four are missing
    // in both, so the sync check passes for cognitive-complexity but reports
    // the other four as missing in both files. Verify the comment-stripped key
    // is NOT reported as mismatched.
    if let Err(errors) = result {
        let joined = errors.join("\n");
        assert!(
            !joined.contains("mismatch for cognitive-complexity-threshold"),
            "comment should not cause a false mismatch: {joined}"
        );
    }
}
