//! Repository-level contract test for the workspace lint configuration.
//!
//! Issue #564: rustc warnings (`unused_imports`, `dead_code`,
//! `unused_variables`, `unused_mut`) were only denied by the clippy CI jobs
//! that pass `-D warnings`. Every other cargo invocation (`build`, `check`,
//! `test`) tolerated them. Denying warnings inside `[lints.rust]` makes every
//! cargo invocation fail on a rustc warning, not just clippy.
//!
//! These tests assert over `Cargo.toml` text (mirroring
//! `tests/coderabbit_policy.rs` / `tests/ocr_review_policy.rs`) so CI fails
//! mechanically if the deny-warnings baseline or the clippy allow-list
//! regresses.

use std::{fs, io, path::Path};

fn repository_text(relative_path: &str) -> io::Result<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not read {}: {error}", path.display()),
        )
    })
}

/// Collect the body of a TOML `[table]` header: the lines after `header` until
/// the next line beginning with `[` at the top level.
fn toml_section<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let needle = format!("[{header}]");
    text.lines()
        .skip_while(|line| line.trim() != needle)
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .collect()
}

fn section_has_setting(section: &[&str], setting: &str) -> bool {
    section
        .iter()
        .any(|line| line.trim() == setting || line.trim().starts_with(&format!("{setting} ")))
}

#[test]
fn lints_rust_denies_all_warnings_and_forbids_unsafe() -> io::Result<()> {
    let manifest = repository_text("Cargo.toml")?;
    let lints_rust = toml_section(&manifest, "lints.rust");

    assert!(
        section_has_setting(&lints_rust, "warnings = \"deny\""),
        "[lints.rust] must set warnings = \"deny\" so every cargo invocation \
         (build/check/test) fails on a rustc warning, not just clippy"
    );
    assert!(
        section_has_setting(&lints_rust, "unsafe_code = \"forbid\""),
        "[lints.rust] must keep unsafe_code = \"forbid\""
    );

    Ok(())
}

#[test]
fn lints_clippy_allow_list_still_suppresses_intended_lints() -> io::Result<()> {
    let manifest = repository_text("Cargo.toml")?;
    let lints_clippy = toml_section(&manifest, "lints.clippy");

    // The [lints.rust] warnings=deny change must not touch the clippy
    // namespace; the deliberately-allowed clippy lints must remain allowed so
    // the existing clippy runs do not regress.
    for allowed in ["needless_pass_by_value", "redundant_clone"] {
        assert!(
            section_has_setting(&lints_clippy, &format!("{allowed} = \"allow\"")),
            "[lints.clippy] must still allow {allowed}; the rust deny-warnings \
             change is scoped to the rust namespace"
        );
    }

    Ok(())
}
