//! Contract tests for the plugin package dependency decision record (issue #389).
//!
//! CW-09 makes an approved dependency decision record a hard entry gate. These
//! tests keep the record and the build in agreement: every crate the record
//! approves must appear in `Cargo.toml` with the exact feature configuration
//! the record justifies, and at the exact version and checksum recorded in
//! `Cargo.lock`. A silent version bump, a re-enabled default feature, or a
//! deleted rationale therefore fails the build rather than drifting.

use std::path::{Path, PathBuf};

const RECORD_PATH: &str = "dev-docs/decisions/plugin-package-dependencies.md";

/// Crates the record approves, with the exact lockfile version and checksum it
/// publishes.
const APPROVED: [(&str, &str, &str); 7] = [
    (
        "flate2",
        "1.1.9",
        "843fba2746e448b37e26a819579957415c8cef339bf08564fe8b7ddbd959573c",
    ),
    (
        "tar",
        "0.4.46",
        "3f6221d9a6003c78398e3b239969f352578258df48c8eb051caadae0015bc840",
    ),
    (
        "miniz_oxide",
        "0.8.9",
        "1fa76a2c86f704bdb222d66965fb3d63269ce38518b83cb0575fca855ebb6316",
    ),
    (
        "crc32fast",
        "1.5.0",
        "9481c1c90cbf2ac953f07c8d4a58aa3945c425b7185c9154d67a65e4230da511",
    ),
    (
        "adler2",
        "2.0.1",
        "320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa",
    ),
    (
        "simd-adler32",
        "0.3.10",
        "3a219298ac11a56ea9a6d2120044824d6f01aeb034955e7af7bc16858527deea",
    ),
    (
        "filetime",
        "0.2.29",
        "5c287a33c7f0a620c38e641e7f60827713987b3c0f26e8ddc9462cc69cf75759",
    ),
];

fn repo_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative.as_ref())
}

fn read(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// The `[[package]]` block for one crate in `Cargo.lock`.
fn lock_entry(lock: &str, name: &str) -> String {
    let needle = format!("name = \"{name}\"");
    let start = lock
        .find(&needle)
        .unwrap_or_else(|| panic!("{name} is absent from Cargo.lock"));
    let rest = &lock[start..];
    let end = rest.find("\n[[package]]").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn the_decision_record_exists_and_names_its_approval() {
    let record = read(RECORD_PATH);
    for required in [
        "Approval date",
        "2026-08-05",
        "Approver",
        "acoliver",
        "Approval discussion",
        "https://github.com/vybestack/llxprt-jefe/issues/389",
    ] {
        assert!(
            record.contains(required),
            "the decision record must publish {required:?}"
        );
    }
}

#[test]
fn every_approved_crate_is_locked_at_its_recorded_version_and_checksum() {
    let record = read(RECORD_PATH);
    let lock = read("Cargo.lock");
    for (name, version, checksum) in APPROVED {
        assert!(
            record.contains(checksum),
            "the record must publish the {name} checksum"
        );
        assert!(
            record.contains(version),
            "the record must publish the {name} version"
        );
        let entry = lock_entry(&lock, name);
        assert!(
            entry.contains(&format!("version = \"{version}\"")),
            "{name} must be locked at {version}"
        );
        assert!(
            entry.contains(&format!("checksum = \"{checksum}\"")),
            "{name} must be locked at the recorded checksum"
        );
    }
}

#[test]
fn the_gzip_backend_stays_pinned_to_safe_rust() {
    let manifest = read("Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("flate2 ="))
        .unwrap_or_else(|| panic!("flate2 must be a direct dependency"));
    assert!(
        line.contains("default-features = false"),
        "flate2 defaults must stay off so the backend cannot drift: {line}"
    );
    assert!(
        line.contains("\"rust_backend\""),
        "flate2 must use the pure-Rust miniz_oxide backend: {line}"
    );
}

#[test]
fn tar_default_features_stay_off() {
    let manifest = read("Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("tar ="))
        .unwrap_or_else(|| panic!("tar must be a direct dependency"));
    assert!(
        line.contains("default-features = false"),
        "tar's default xattr feature must stay off: {line}"
    );
}

#[test]
fn no_second_semver_or_sha256_implementation_is_introduced() {
    let manifest = read("Cargo.toml");
    for rejected in ["semver =", "sha2 =", "sha256 =", "ring =", "openssl ="] {
        assert!(
            !manifest
                .lines()
                .any(|line| line.trim_start().starts_with(rejected)),
            "{rejected} duplicates an in-tree implementation the record keeps"
        );
    }
}

#[test]
fn the_record_justifies_rejecting_local_codecs_and_host_commands() {
    let record = read(RECORD_PATH);
    for required in [
        "Implementing gzip, tar, SemVer, or SHA-256 locally",
        "Invoking external",
        "CanonicalSemver",
        "domain::sha256",
        "Process-group helper",
    ] {
        assert!(
            record.contains(required),
            "the record must carry its {required:?} rationale"
        );
    }
}

#[test]
fn the_record_proves_each_required_rejection_is_testable() {
    let record = read(RECORD_PATH);
    for required in [
        "MultiGzDecoder",
        "path_bytes",
        "entry_type",
        "Header::size()",
        "PAX",
        "known-answer",
    ] {
        assert!(
            record.contains(required),
            "the record must show how {required:?} is reachable through the selected APIs"
        );
    }
}
