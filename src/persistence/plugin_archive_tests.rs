//! Adversarial archive table (issue #389 CW-09, acceptance rows A1–A5 and A8).

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{EntryType, Header};

use super::*;

/// One entry to place in a test archive.
struct Entry {
    path: &'static str,
    kind: EntryType,
    body: Vec<u8>,
    mode: u32,
    link: Option<&'static str>,
    /// Size to declare in the header, when it should disagree with the body.
    declared_size: Option<u64>,
}

impl Entry {
    fn file(path: &'static str, body: &str) -> Self {
        Self {
            path,
            kind: EntryType::Regular,
            body: body.as_bytes().to_vec(),
            mode: 0o644,
            link: None,
            declared_size: None,
        }
    }

    fn directory(path: &'static str) -> Self {
        Self {
            path,
            kind: EntryType::Directory,
            body: Vec::new(),
            mode: 0o755,
            link: None,
            declared_size: None,
        }
    }

    const fn kind(mut self, kind: EntryType) -> Self {
        self.kind = kind;
        self
    }

    const fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    const fn link(mut self, target: &'static str) -> Self {
        self.link = Some(target);
        self
    }
}

/// A well-formed manifest for the standard fixture package.
fn manifest_body() -> String {
    r#"{
      "manifest_schema": 1,
      "id": "vendor.pkg",
      "version": "1.0.0",
      "display_name": "Pkg",
      "host_api": { "minimum": "1.0.0", "maximum": "1.0.0" },
      "protocol": 1,
      "provider": { "mode": "none", "binaries": {} },
      "actions": [],
      "panels": [],
      "routes": [],
      "screens": []
    }"#
    .to_owned()
}

/// Build one uncompressed tar containing `entries`.
fn tar_bytes(entries: Vec<Entry>) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for entry in entries {
        let mut header = Header::new_ustar();
        header.set_entry_type(entry.kind);
        header.set_mode(entry.mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        let size = entry.declared_size.unwrap_or(entry.body.len() as u64);
        header.set_size(if entry.kind == EntryType::Regular {
            size
        } else {
            0
        });
        if let Some(target) = entry.link {
            header
                .set_link_name(target)
                .unwrap_or_else(|error| panic!("link name must set: {error}"));
        }
        header.set_cksum();
        builder
            .append_data(&mut header, entry.path, entry.body.as_slice())
            .unwrap_or_else(|error| panic!("entry must append: {error}"));
    }
    builder
        .into_inner()
        .unwrap_or_else(|error| panic!("tar must finish: {error}"))
}

/// Compress `raw` as one gzip member.
fn gzip(raw: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(raw)
        .unwrap_or_else(|error| panic!("gzip must write: {error}"));
    encoder
        .finish()
        .unwrap_or_else(|error| panic!("gzip must finish: {error}"))
}

/// A valid archive of the standard fixture package plus `extra` entries.
fn archive_with(extra: Vec<Entry>) -> Vec<u8> {
    let mut entries = vec![
        Entry::directory("vendor.pkg-1.0.0/"),
        Entry::file("vendor.pkg-1.0.0/plugin.json", &manifest_body()),
    ];
    entries.extend(extra);
    gzip(&tar_bytes(entries))
}

fn read(bytes: &[u8]) -> Result<ArchiveContents, ArchiveError> {
    read_archive(bytes)
}

fn rejected(bytes: &[u8]) -> ArchiveError {
    read(bytes)
        .err()
        .unwrap_or_else(|| panic!("the archive must be rejected"))
}

#[test]
fn a_well_formed_archive_yields_its_package_and_entries() {
    let contents = read(&archive_with(vec![Entry::file(
        "vendor.pkg-1.0.0/resources/help.txt",
        "help",
    )]))
    .unwrap_or_else(|error| panic!("archive must read: {error}"));

    assert_eq!(contents.coordinate().to_string(), "vendor.pkg@1.0.0");
    assert_eq!(contents.manifest().display_name(), "Pkg");
    let paths: Vec<&str> = contents
        .files()
        .iter()
        .map(|file| file.path().as_str())
        .collect();
    assert_eq!(paths, vec!["plugin.json", "resources/help.txt"]);
}

#[test]
fn the_digest_covers_the_exact_archive_bytes() {
    let bytes = archive_with(Vec::new());
    let contents = read(&bytes).unwrap_or_else(|error| panic!("archive must read: {error}"));
    assert_eq!(
        contents.digest(),
        crate::domain::sha256::Sha256::digest(&bytes)
    );
}

#[test]
fn input_that_is_not_gzip_is_rejected() {
    for bytes in [b"not gzip at all".to_vec(), Vec::new(), vec![0x1f, 0x8b]] {
        assert!(
            matches!(rejected(&bytes), ArchiveError::Gzip { .. }),
            "non-gzip input must be rejected"
        );
    }
}

#[test]
fn a_corrupt_gzip_checksum_is_rejected() {
    let mut bytes = archive_with(Vec::new());
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    assert!(matches!(rejected(&bytes), ArchiveError::Gzip { .. }));
}

#[test]
fn concatenated_gzip_members_are_rejected() {
    let mut bytes = archive_with(Vec::new());
    bytes.extend(archive_with(Vec::new()));
    assert_eq!(rejected(&bytes), ArchiveError::TrailingBytes);
}

#[test]
fn trailing_bytes_after_the_gzip_member_are_rejected() {
    let mut bytes = archive_with(Vec::new());
    bytes.extend_from_slice(b"tacked on");
    assert_eq!(rejected(&bytes), ArchiveError::TrailingBytes);
}

#[test]
fn links_devices_and_special_files_are_rejected_by_entry_type() {
    for (kind, link) in [
        (EntryType::Symlink, Some("../../etc/passwd")),
        (EntryType::Link, Some("vendor.pkg-1.0.0/plugin.json")),
        (EntryType::Char, None),
        (EntryType::Block, None),
        (EntryType::Fifo, None),
        (EntryType::Continuous, None),
    ] {
        let mut entry = Entry::file("vendor.pkg-1.0.0/thing", "").kind(kind);
        if let Some(target) = link {
            entry = entry.link(target);
        }
        let error = rejected(&archive_with(vec![entry]));
        assert!(
            matches!(error, ArchiveError::ForbiddenEntry { .. }),
            "{kind:?} must be rejected by type, got {error}"
        );
    }
}

#[test]
fn sparse_and_extension_entries_never_reach_the_package() {
    // These are rejected either by the tar layer, which refuses a malformed
    // extension header outright, or by the entry-type gate once it surfaces as
    // an entry. Either way no package is produced, which is the contract; the
    // exact layer is not something the archive contract should pin.
    for kind in [
        EntryType::GNUSparse,
        EntryType::XGlobalHeader,
        EntryType::GNULongName,
        EntryType::GNULongLink,
    ] {
        let entry = Entry::file("vendor.pkg-1.0.0/thing", "").kind(kind);
        let error = rejected(&archive_with(vec![entry]));
        assert!(
            matches!(
                error,
                ArchiveError::ForbiddenEntry { .. } | ArchiveError::Tar { .. }
            ),
            "{kind:?} must never yield a package, got {error}"
        );
    }
}

#[test]
fn a_path_outside_the_single_root_directory_is_rejected() {
    let error = rejected(&archive_with(vec![Entry::file("elsewhere/file", "x")]));
    assert!(
        matches!(error, ArchiveError::OutsideRoot { .. }),
        "an entry outside the root must be rejected, got {error}"
    );
}

#[test]
fn an_archive_without_exactly_one_root_directory_is_rejected() {
    let empty = gzip(&tar_bytes(Vec::new()));
    assert_eq!(rejected(&empty), ArchiveError::NoRootDirectory);

    let two = gzip(&tar_bytes(vec![
        Entry::directory("vendor.pkg-1.0.0/"),
        Entry::file("vendor.pkg-1.0.0/plugin.json", &manifest_body()),
        Entry::directory("other.pkg-1.0.0/"),
        Entry::file("other.pkg-1.0.0/plugin.json", &manifest_body()),
    ]));
    assert!(
        matches!(rejected(&two), ArchiveError::OutsideRoot { .. }),
        "a second root directory must be rejected"
    );
}

#[test]
fn a_root_directory_that_is_not_id_dash_version_is_rejected() {
    let bytes = gzip(&tar_bytes(vec![
        Entry::directory("notapackage/"),
        Entry::file("notapackage/plugin.json", &manifest_body()),
    ]));
    assert!(
        matches!(rejected(&bytes), ArchiveError::RootName { .. }),
        "the root must be named <plugin-id>-<version>"
    );
}

#[test]
fn a_manifest_identity_that_contradicts_the_root_name_is_rejected() {
    let bytes = gzip(&tar_bytes(vec![
        Entry::directory("vendor.pkg-2.0.0/"),
        Entry::file("vendor.pkg-2.0.0/plugin.json", &manifest_body()),
    ]));
    assert_eq!(
        rejected(&bytes),
        ArchiveError::IdentityMismatch {
            root: "vendor.pkg@2.0.0".to_owned(),
            manifest: "vendor.pkg@1.0.0".to_owned()
        }
    );
}

#[test]
fn an_archive_without_a_manifest_is_rejected() {
    let bytes = gzip(&tar_bytes(vec![
        Entry::directory("vendor.pkg-1.0.0/"),
        Entry::file("vendor.pkg-1.0.0/other.txt", "x"),
    ]));
    assert_eq!(rejected(&bytes), ArchiveError::MissingManifest);
}

#[test]
fn a_forbidden_path_shape_is_rejected() {
    // `..` and `.` components cannot even be built by a conforming tar writer,
    // so the shapes exercised here are the ones a writer will happily emit but
    // the package contract still refuses: a backslash separator, excess depth,
    // and an over-long path.
    let deep = format!("vendor.pkg-1.0.0/{}", vec!["d"; 17].join("/"));
    let long = format!("vendor.pkg-1.0.0/{}", "n".repeat(1_025));
    for path in [r"vendor.pkg-1.0.0/a\b".to_owned(), deep, long] {
        let bytes = gzip(&tar_bytes(vec![
            Entry::directory("vendor.pkg-1.0.0/"),
            Entry::file("vendor.pkg-1.0.0/plugin.json", &manifest_body()),
            Entry {
                path: Box::leak(path.clone().into_boxed_str()),
                ..Entry::file("placeholder", "x")
            },
        ]));
        let error = rejected(&bytes);
        assert!(
            matches!(
                error,
                ArchiveError::ForbiddenPath { .. } | ArchiveError::OutsideRoot { .. }
            ),
            "{path} must be rejected, got {error}"
        );
    }
}

#[test]
fn a_duplicate_normalized_path_is_rejected() {
    let bytes = archive_with(vec![
        Entry::file("vendor.pkg-1.0.0/dup.txt", "first"),
        Entry::file("vendor.pkg-1.0.0/dup.txt", "second"),
    ]);
    assert_eq!(
        rejected(&bytes),
        ArchiveError::DuplicatePath {
            path: "dup.txt".to_owned()
        }
    );
}

#[test]
fn a_case_folding_duplicate_is_rejected() {
    // On a case-insensitive filesystem these two entries name one file, so the
    // archive is ambiguous regardless of the host it is unpacked on.
    let bytes = archive_with(vec![
        Entry::file("vendor.pkg-1.0.0/Readme.txt", "first"),
        Entry::file("vendor.pkg-1.0.0/README.TXT", "second"),
    ]);
    assert_eq!(
        rejected(&bytes),
        ArchiveError::CaseFoldDuplicate {
            path: "README.TXT".to_owned()
        }
    );
}

#[test]
fn an_entry_exceeding_the_per_file_bound_is_rejected() {
    let big = "x".repeat(MANIFEST_BYTE_LIMIT + 1);
    let bytes = archive_with(vec![Entry::file("vendor.pkg-1.0.0/big.bin", &big)]);
    assert!(
        matches!(rejected(&bytes), ArchiveError::FileTooLarge { .. }),
        "a file over the per-file bound must be rejected"
    );
}

#[test]
fn a_header_size_over_the_per_file_bound_is_rejected_before_the_body_is_read() {
    // The header lies about a small body. The bound is enforced from the
    // declared size, so the body is never read at all.
    let mut entry = Entry::file("vendor.pkg-1.0.0/lying.bin", "small");
    entry.declared_size = Some(u64::try_from(MANIFEST_BYTE_LIMIT).unwrap_or(u64::MAX) + 1);
    let bytes = archive_with(vec![entry]);
    assert!(matches!(
        rejected(&bytes),
        ArchiveError::FileTooLarge { .. }
    ));
}

#[test]
fn exceeding_the_total_expanded_bound_is_rejected() {
    // Each file is inside the per-file bound; together they cross the total.
    let chunk = "x".repeat(MANIFEST_BYTE_LIMIT);
    let count = usize::try_from(ARCHIVE_EXPANDED_BYTE_LIMIT).unwrap_or(usize::MAX)
        / MANIFEST_BYTE_LIMIT
        + 1;
    let extra: Vec<Entry> = (0..count)
        .map(|index| Entry {
            path: Box::leak(format!("vendor.pkg-1.0.0/f{index}.bin").into_boxed_str()),
            ..Entry::file("placeholder", &chunk)
        })
        .collect();
    assert!(
        matches!(
            rejected(&archive_with(extra)),
            ArchiveError::ExpandedTooLarge { .. }
        ),
        "the total expanded bound must be enforced"
    );
}

#[test]
fn exceeding_the_entry_count_bound_is_rejected() {
    let extra: Vec<Entry> = (0..=ARCHIVE_ENTRY_LIMIT)
        .map(|index| Entry {
            path: Box::leak(format!("vendor.pkg-1.0.0/f{index}.txt").into_boxed_str()),
            ..Entry::file("placeholder", "x")
        })
        .collect();
    assert!(
        matches!(
            rejected(&archive_with(extra)),
            ArchiveError::TooManyEntries { .. }
        ),
        "the entry-count bound must be enforced"
    );
}

#[test]
fn a_truncated_archive_is_rejected() {
    let bytes = archive_with(vec![Entry::file("vendor.pkg-1.0.0/a.txt", "hello")]);
    let cut = &bytes[..bytes.len() / 2];
    let error = rejected(cut);
    assert!(
        matches!(error, ArchiveError::Gzip { .. } | ArchiveError::Tar { .. }),
        "a truncated archive must be rejected, got {error}"
    );
}

#[test]
fn setuid_setgid_and_sticky_bits_are_cleared_and_modes_are_normalized() {
    let bytes = archive_with(vec![
        Entry::file("vendor.pkg-1.0.0/hot.bin", "x").mode(0o4777),
        Entry::file("vendor.pkg-1.0.0/plain.txt", "x").mode(0o600),
    ]);
    let contents = read(&bytes).unwrap_or_else(|error| panic!("archive must read: {error}"));
    for file in contents.files() {
        assert!(
            file.mode() == 0o644 || file.mode() == 0o755,
            "{} must be normalized, got {:o}",
            file.path(),
            file.mode()
        );
        assert_eq!(
            file.mode() & 0o7000,
            0,
            "{} must have no setuid, setgid or sticky bit",
            file.path()
        );
    }
}

#[test]
fn an_executable_archive_entry_keeps_an_executable_normalized_mode() {
    let bytes = archive_with(vec![
        Entry::file("vendor.pkg-1.0.0/bin/provider", "x").mode(0o755),
        Entry::file("vendor.pkg-1.0.0/resources/help.txt", "x").mode(0o644),
    ]);
    let contents = read(&bytes).unwrap_or_else(|error| panic!("archive must read: {error}"));
    let mode_of = |wanted: &str| {
        contents
            .files()
            .iter()
            .find(|file| file.path().as_str() == wanted)
            .map(ArchiveFile::mode)
    };
    assert_eq!(mode_of("bin/provider"), Some(0o755));
    assert_eq!(mode_of("resources/help.txt"), Some(0o644));
}
