//! Multiplexer provenance verification (issue #540 slice S7/V7).
//!
//! CI downloads a pinned psmux archive and refuses it unless the SHA256 matches.
//! User machines verify nothing beyond parsing `-V`, so whatever `psmux.exe`
//! happens to be first on `PATH` is trusted.
//!
//! The archive digest cannot be applied directly to a binary on `PATH`: it
//! covers the zip, not the extracted executable, and a user's psmux may have
//! arrived by another route entirely. So the guarantee jefe can actually make
//! is narrower and stated as such -- it records the fingerprint of the binary it
//! qualified, refuses one it has never qualified, and detects the binary
//! changing underneath a running process.
//!
//! SHA-256 is implemented in-crate rather than pulled in as a dependency, and
//! is checked against the published NIST vectors below.

use jefe::runtime::{
    BinaryFingerprint, PINNED_PSMUX_ARCHIVE_SHA256, ProvenanceManifest, ProvenanceVerdict,
    sha256_hex,
};

/// Published NIST vectors. A hand-rolled digest that is not checked against
/// them is worth less than no check at all, because it would fail closed on
/// correct binaries or open on wrong ones.
#[test]
fn the_digest_matches_the_published_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    );
}

/// Multi-block input exercises the message schedule across chunk boundaries,
/// which a single-block implementation would pass while still being wrong.
#[test]
fn the_digest_spans_block_boundaries() {
    assert_eq!(
        sha256_hex(&b"a".repeat(1_000_000))[..16],
        *"cdc76e5c9914fb92",
    );
}

/// The CI-pinned archive digest is carried so provenance can be stated rather
/// than implied, and must be the value CI actually enforces.
#[test]
fn the_pinned_archive_digest_is_recorded() {
    assert_eq!(
        PINNED_PSMUX_ARCHIVE_SHA256,
        "60ff7b236f64184921cef3c1ff2611aa5a36fcc7ed8e2a58e968b8ded57f6028",
    );
}

/// A binary jefe has qualified is accepted.
#[test]
fn a_qualified_binary_is_accepted() {
    let fingerprint = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"psmux"));
    let manifest = ProvenanceManifest::with_qualified([fingerprint.sha256().to_owned()]);

    assert_eq!(manifest.verify(&fingerprint), ProvenanceVerdict::Qualified);
}

/// An unrecognised binary is refused, and the refusal must name the path and
/// the digest -- a verdict the operator cannot act on is not a failure mode.
#[test]
fn an_unqualified_binary_is_refused_with_something_to_act_on() {
    let fingerprint = BinaryFingerprint::new("C:/other/psmux.exe", 99, sha256_hex(b"stranger"));
    let manifest = ProvenanceManifest::with_qualified([sha256_hex(b"psmux")]);

    let verdict = manifest.verify(&fingerprint);
    let ProvenanceVerdict::Unqualified { diagnostic } = verdict else {
        panic!("an unknown binary must be refused, got {verdict:?}");
    };

    assert!(diagnostic.contains("C:/other/psmux.exe"), "{diagnostic}");
    assert!(
        diagnostic.contains(&sha256_hex(b"stranger")),
        "{diagnostic}"
    );
}

/// The case the issue calls out: the binary is replaced while jefe is running.
/// Comparing against what was qualified at startup is what makes that visible.
#[test]
fn a_binary_swapped_underneath_a_running_jefe_is_detected() {
    let qualified = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"psmux"));
    let now = BinaryFingerprint::new("C:/tools/psmux.exe", 2048, sha256_hex(b"replacement"));

    let verdict = qualified.detect_change(&now);
    let ProvenanceVerdict::Changed { diagnostic } = verdict else {
        panic!("a replaced binary must be detected, got {verdict:?}");
    };

    assert!(diagnostic.contains("C:/tools/psmux.exe"), "{diagnostic}");
    assert!(
        diagnostic.contains(&sha256_hex(b"psmux"))
            && diagnostic.contains(&sha256_hex(b"replacement")),
        "the diagnostic must show both digests so the change is legible: {diagnostic}",
    );
}

/// Re-reading the same unchanged binary must not be reported as a change, or
/// the check would fire constantly and be ignored.
#[test]
fn an_unchanged_binary_is_not_reported_as_changed() {
    let qualified = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"psmux"));
    let again = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"psmux"));

    assert_eq!(
        qualified.detect_change(&again),
        ProvenanceVerdict::Qualified
    );
}

/// Length is part of the fingerprint, but the digest decides. A same-length
/// replacement must still be caught.
#[test]
fn a_same_length_replacement_is_still_caught() {
    let qualified = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"psmux"));
    let swapped = BinaryFingerprint::new("C:/tools/psmux.exe", 1024, sha256_hex(b"other"));

    assert!(matches!(
        qualified.detect_change(&swapped),
        ProvenanceVerdict::Changed { .. },
    ));
}
