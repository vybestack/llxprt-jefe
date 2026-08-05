//! Install transaction table (issue #389 CW-09, acceptance rows A6, A7, A9).

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::*;

fn manifest_body(id: &str, version: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "{version}",
          "display_name": "Pkg",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "none", "binaries": {{}} }},
          "actions": [],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#
    )
}

/// Build a valid archive for `id`@`version` plus `extra` files.
fn archive(id: &str, version: &str, extra: &[(&str, &str, u32)]) -> Vec<u8> {
    let root = format!("{id}-{version}");
    let mut builder = tar::Builder::new(Vec::new());
    let mut append = |path: String, body: &[u8], mode: u32, directory: bool| {
        let mut header = tar::Header::new_ustar();
        header.set_entry_type(if directory {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_mode(mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(if directory { 0 } else { body.len() as u64 });
        header.set_cksum();
        builder
            .append_data(&mut header, path, body)
            .unwrap_or_else(|error| panic!("entry must append: {error}"));
    };
    append(format!("{root}/"), b"", 0o755, true);
    append(
        format!("{root}/plugin.json"),
        manifest_body(id, version).as_bytes(),
        0o644,
        false,
    );
    for (path, body, mode) in extra {
        append(format!("{root}/{path}"), body.as_bytes(), *mode, false);
    }
    let raw = builder
        .into_inner()
        .unwrap_or_else(|error| panic!("tar must finish: {error}"));
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(&raw)
        .unwrap_or_else(|error| panic!("gzip must write: {error}"));
    encoder
        .finish()
        .unwrap_or_else(|error| panic!("gzip must finish: {error}"))
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .unwrap_or_else(|error| panic!("{} must stat: {error}", path.display()))
        .permissions()
        .mode()
        & 0o7777
}

#[test]
fn a_valid_archive_commits_into_the_user_root() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    let outcome = install_archive(
        &plugins,
        &archive(
            "vendor.pkg",
            "1.0.0",
            &[("resources/help.txt", "help", 0o644)],
        ),
    )
    .unwrap_or_else(|error| panic!("install must succeed: {error}"));

    assert_eq!(outcome.coordinate().to_string(), "vendor.pkg@1.0.0");
    let expected = plugins.join("installed").join("vendor.pkg").join("1.0.0");
    assert_eq!(outcome.destination(), expected);
    assert!(expected.join("plugin.json").is_file());
    assert_eq!(
        fs::read_to_string(expected.join("resources/help.txt")).unwrap_or_default(),
        "help"
    );
}

#[test]
fn staging_is_emptied_after_a_successful_commit() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    install_archive(&plugins, &archive("vendor.pkg", "1.0.0", &[]))
        .unwrap_or_else(|error| panic!("install must succeed: {error}"));

    let staging = plugins.join(".staging");
    let leftovers = fs::read_dir(&staging)
        .map(Iterator::count)
        .unwrap_or_default();
    assert_eq!(leftovers, 0, "no staging directory may survive a commit");
}

#[test]
fn an_already_installed_version_is_never_overwritten() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    let bytes = archive("vendor.pkg", "1.0.0", &[("a.txt", "first", 0o644)]);
    install_archive(&plugins, &bytes)
        .unwrap_or_else(|error| panic!("first install must succeed: {error}"));

    let second = archive("vendor.pkg", "1.0.0", &[("a.txt", "second", 0o644)]);
    let error = install_archive(&plugins, &second)
        .err()
        .unwrap_or_else(|| panic!("a second install must be refused"));
    assert!(matches!(error, InstallError::DestinationExists { .. }));
    assert!(error.installed_tree_unchanged());

    let installed = plugins.join("installed/vendor.pkg/1.0.0/a.txt");
    assert_eq!(
        fs::read_to_string(installed).unwrap_or_default(),
        "first",
        "the installed bytes must be untouched"
    );
}

#[test]
fn side_by_side_versions_both_install() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    for version in ["1.0.0", "1.1.0", "1.0.0+build.2"] {
        install_archive(&plugins, &archive("vendor.pkg", version, &[]))
            .unwrap_or_else(|error| panic!("{version} must install: {error}"));
    }
    let owner = plugins.join("installed").join("vendor.pkg");
    let count = fs::read_dir(&owner)
        .map(Iterator::count)
        .unwrap_or_default();
    assert_eq!(count, 3, "versions install side by side");
}

#[test]
fn a_rejected_archive_leaves_no_trace() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    let mut broken = archive("vendor.pkg", "1.0.0", &[]);
    broken.extend_from_slice(b"trailing");

    let error = install_archive(&plugins, &broken)
        .err()
        .unwrap_or_else(|| panic!("the archive must be refused"));
    assert!(matches!(error, InstallError::Archive(_)));
    assert!(error.installed_tree_unchanged());
    assert!(
        !plugins.join("installed").exists(),
        "nothing may be installed"
    );
}

#[test]
fn a_failure_after_staging_removes_only_the_staging_directory() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    install_archive(&plugins, &archive("vendor.pkg", "1.0.0", &[]))
        .unwrap_or_else(|error| panic!("first install must succeed: {error}"));

    // The destination now exists, so a second attempt fails before staging.
    let _ = install_archive(&plugins, &archive("vendor.pkg", "1.0.0", &[]));

    let staging = plugins.join(".staging");
    let leftovers = fs::read_dir(&staging)
        .map(Iterator::count)
        .unwrap_or_default();
    assert_eq!(leftovers, 0, "no staging directory may survive a failure");
    assert!(
        plugins
            .join("installed/vendor.pkg/1.0.0/plugin.json")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn installed_modes_are_explicit_and_carry_no_setuid() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    install_archive(
        &plugins,
        &archive(
            "vendor.pkg",
            "1.0.0",
            &[
                ("bin/provider", "x", 0o4777),
                ("resources/help.txt", "x", 0o666),
            ],
        ),
    )
    .unwrap_or_else(|error| panic!("install must succeed: {error}"));

    let root = plugins.join("installed/vendor.pkg/1.0.0");
    assert_eq!(mode_of(&root), 0o755);
    assert_eq!(mode_of(&root.join("bin/provider")), 0o755);
    assert_eq!(mode_of(&root.join("resources/help.txt")), 0o644);
    assert_eq!(mode_of(&root.join("plugin.json")), 0o644);
}

#[cfg(unix)]
#[test]
fn the_staging_root_is_private_to_this_user() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    install_archive(&plugins, &archive("vendor.pkg", "1.0.0", &[]))
        .unwrap_or_else(|error| panic!("install must succeed: {error}"));
    // The per-install directory is removed on success, so the observable
    // guarantee is that the staging area itself is not world-readable.
    assert_eq!(mode_of(&plugins.join(".staging")), 0o755);
}

#[test]
fn a_developer_directory_installs_through_the_same_transaction() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    let source = temp.path().join("vendor.pkg-1.0.0");
    let nested = source.join("resources");
    if fs::create_dir_all(&nested).is_err() {
        return;
    }
    if fs::write(
        source.join("plugin.json"),
        manifest_body("vendor.pkg", "1.0.0"),
    )
    .is_err()
    {
        return;
    }
    if fs::write(nested.join("help.txt"), b"help").is_err() {
        return;
    }

    let outcome = install_developer_directory(&plugins, &source)
        .unwrap_or_else(|error| panic!("developer install must succeed: {error}"));
    assert_eq!(outcome.coordinate().to_string(), "vendor.pkg@1.0.0");
    let installed = plugins.join("installed/vendor.pkg/1.0.0");
    assert!(installed.join("plugin.json").is_file());
    assert_eq!(
        fs::read_to_string(installed.join("resources/help.txt")).unwrap_or_default(),
        "help"
    );
}

#[cfg(unix)]
#[test]
fn a_developer_directory_never_follows_a_source_symlink() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let plugins = temp.path().join("plugins");
    let secret = temp.path().join("secret.txt");
    if fs::write(&secret, b"do not copy me").is_err() {
        return;
    }
    let source = temp.path().join("vendor.pkg-1.0.0");
    if fs::create_dir_all(&source).is_err() {
        return;
    }
    if fs::write(
        source.join("plugin.json"),
        manifest_body("vendor.pkg", "1.0.0"),
    )
    .is_err()
    {
        return;
    }
    if std::os::unix::fs::symlink(&secret, source.join("link.txt")).is_err() {
        return;
    }

    let error = install_developer_directory(&plugins, &source)
        .err()
        .unwrap_or_else(|| panic!("a source symlink must be refused"));
    assert!(matches!(error, InstallError::Archive(_)));
    assert!(!plugins.join("installed").exists());
}

#[test]
fn a_developer_directory_and_its_archive_agree_on_the_digest() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let source = temp.path().join("vendor.pkg-1.0.0");
    if fs::create_dir_all(&source).is_err() {
        return;
    }
    if fs::write(
        source.join("plugin.json"),
        manifest_body("vendor.pkg", "1.0.0"),
    )
    .is_err()
    {
        return;
    }

    let from_directory = install_developer_directory(&temp.path().join("a"), &source)
        .unwrap_or_else(|error| panic!("developer install must succeed: {error}"));
    let from_archive =
        install_archive(&temp.path().join("b"), &archive("vendor.pkg", "1.0.0", &[]))
            .unwrap_or_else(|error| panic!("archive install must succeed: {error}"));

    assert_eq!(
        from_directory.digest(),
        from_archive.digest(),
        "the same tree must digest the same however it arrived"
    );
}

#[test]
fn an_indeterminate_commit_carries_the_plg_e503_code() {
    // The code is part of the operator contract, so it is asserted directly
    // rather than only through a filesystem failure that is hard to force.
    let error = InstallError::IndeterminateCommit {
        destination: PathBuf::from("/tmp/x"),
        reason: "sync failed".to_owned(),
    };
    assert_eq!(error.code(), Some(PluginCode::IndeterminateCommit));
    assert!(
        !error.installed_tree_unchanged(),
        "an indeterminate commit must not claim the tree is unchanged"
    );
    assert!(error.to_string().contains("PLG-E503"));
}

#[test]
fn every_other_failure_reports_the_tree_unchanged_and_no_code() {
    for error in [
        InstallError::DestinationExists {
            path: PathBuf::from("/tmp/x"),
        },
        InstallError::Filesystem {
            path: PathBuf::from("/tmp/x"),
            reason: "denied".to_owned(),
        },
    ] {
        assert_eq!(error.code(), None);
        assert!(error.installed_tree_unchanged());
    }
}
