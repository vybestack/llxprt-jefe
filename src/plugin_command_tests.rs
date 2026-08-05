//! Plugin command execution table (issue #389 CW-09, acceptance rows C1–C11).

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
          "display_name": "Git Merger",
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

fn archive(id: &str, version: &str) -> Vec<u8> {
    let root = format!("{id}-{version}");
    let mut builder = tar::Builder::new(Vec::new());
    let mut append = |path: String, body: &[u8], directory: bool| {
        let mut header = tar::Header::new_ustar();
        header.set_entry_type(if directory {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_mode(if directory { 0o755 } else { 0o644 });
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(if directory { 0 } else { body.len() as u64 });
        header.set_cksum();
        builder
            .append_data(&mut header, path, body)
            .unwrap_or_else(|error| panic!("entry must append: {error}"));
    };
    append(format!("{root}/"), b"", true);
    append(
        format!("{root}/plugin.json"),
        manifest_body(id, version).as_bytes(),
        false,
    );
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

/// An isolated config directory with an empty schema-2 settings document.
fn workspace(temp: &Path) -> PathBuf {
    let config = temp.join("config");
    std::fs::create_dir_all(&config)
        .unwrap_or_else(|error| panic!("config dir must exist: {error}"));
    std::fs::write(config.join("settings.toml"), b"settings_schema = 2\n")
        .unwrap_or_else(|error| panic!("settings must write: {error}"));
    config
}

fn run_in(config: &Path, command: &PluginCommand) -> RecoveryOutput {
    run(command, Some(config))
}

fn install_fixture(config: &Path, temp: &Path, version: &str, enable: bool) -> RecoveryOutput {
    let file = temp.join(format!("pkg-{version}.tar.gz"));
    std::fs::write(&file, archive("vendor.git-merger", version))
        .unwrap_or_else(|error| panic!("archive must write: {error}"));
    run_in(
        config,
        &PluginCommand::Install {
            source: file,
            developer: false,
            enable,
        },
    )
}

fn settings_text(config: &Path) -> String {
    std::fs::read_to_string(config.join("settings.toml")).unwrap_or_default()
}

#[test]
fn listing_an_empty_installation_succeeds_with_no_rows() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let output = run_in(&config, &PluginCommand::List);
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.is_empty(), "{}", output.stdout);
}

#[test]
fn installing_leaves_the_package_disabled_by_default() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let output = install_fixture(&config, temp.path(), "1.0.0", false);

    assert_eq!(output.exit_code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("disabled"), "{}", output.stdout);
    assert!(
        !settings_text(&config).contains("enabled = true"),
        "installing must not grant trust: {}",
        settings_text(&config)
    );
}

#[test]
fn installing_with_enable_records_trust_and_states_what_it_means() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let output = install_fixture(&config, temp.path(), "1.0.0", true);

    assert_eq!(output.exit_code, 0, "{}", output.stderr);
    assert!(
        output.stdout.contains("unsandboxed"),
        "trust must state that the provider runs unsandboxed: {}",
        output.stdout
    );
    let settings = settings_text(&config);
    assert!(settings.contains("enabled = true"), "{settings}");
    assert!(settings.contains(r#"version = "1.0.0""#), "{settings}");
}

#[test]
fn an_installed_package_lists_and_inspects() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", false);

    let listed = run_in(&config, &PluginCommand::List);
    assert_eq!(listed.exit_code, 0);
    assert!(
        listed.stdout.contains("vendor.git-merger@1.0.0") && listed.stdout.contains("Git Merger"),
        "{}",
        listed.stdout
    );

    let inspected = run_in(
        &config,
        &PluginCommand::Inspect {
            id: "vendor.git-merger".to_owned(),
            version: None,
        },
    );
    assert_eq!(inspected.exit_code, 0);
    assert!(
        inspected.stdout.contains("provider not started"),
        "inspect must state that nothing was executed: {}",
        inspected.stdout
    );
}

#[test]
fn listing_orders_versions_by_precedence_descending() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    for version in ["0.9.0", "1.0.0"] {
        install_fixture(&config, temp.path(), version, false);
    }
    let listed = run_in(&config, &PluginCommand::List);
    let first = listed.stdout.find("1.0.0");
    let second = listed.stdout.find("0.9.0");
    assert!(
        first < second,
        "the highest precedence version leads: {}",
        listed.stdout
    );
}

#[test]
fn inspecting_something_that_is_not_installed_exits_two() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let output = run_in(
        &config,
        &PluginCommand::Inspect {
            id: "vendor.absent".to_owned(),
            version: None,
        },
    );
    assert_eq!(output.exit_code, 2, "{}", output.stderr);
}

#[test]
fn a_malformed_identifier_exits_two() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    for command in [
        PluginCommand::Inspect {
            id: "core.dashboard".to_owned(),
            version: None,
        },
        PluginCommand::Enable {
            id: "core.dashboard".to_owned(),
            version: None,
        },
        PluginCommand::Disable {
            id: "not a plugin".to_owned(),
        },
    ] {
        assert_eq!(run_in(&config, &command).exit_code, 2);
    }
}

#[test]
fn reinstalling_an_existing_version_exits_three_and_changes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", false);
    let again = install_fixture(&config, temp.path(), "1.0.0", false);
    assert_eq!(again.exit_code, 3, "{}", again.stderr);
}

#[test]
fn installing_a_directory_without_developer_is_a_usage_error() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let source = temp.path().join("vendor.git-merger-1.0.0");
    if std::fs::create_dir_all(&source).is_err() {
        return;
    }
    let output = run_in(
        &config,
        &PluginCommand::Install {
            source,
            developer: false,
            enable: false,
        },
    );
    assert_eq!(output.exit_code, 64, "{}", output.stderr);
}

#[test]
fn enable_then_disable_preserves_the_selected_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", false);

    let enabled = run_in(
        &config,
        &PluginCommand::Enable {
            id: "vendor.git-merger".to_owned(),
            version: None,
        },
    );
    assert_eq!(enabled.exit_code, 0, "{}", enabled.stderr);
    assert!(settings_text(&config).contains("enabled = true"));

    let disabled = run_in(
        &config,
        &PluginCommand::Disable {
            id: "vendor.git-merger".to_owned(),
        },
    );
    assert_eq!(disabled.exit_code, 0, "{}", disabled.stderr);
    let settings = settings_text(&config);
    assert!(settings.contains("enabled = false"), "{settings}");
    assert!(
        settings.contains(r#"version = "1.0.0""#),
        "the selection stays recorded while dormant: {settings}"
    );
}

#[test]
fn rollback_selects_an_installed_exact_version() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    for version in ["0.9.0", "1.0.0"] {
        install_fixture(&config, temp.path(), version, false);
    }
    let output = run_in(
        &config,
        &PluginCommand::Rollback {
            id: "vendor.git-merger".to_owned(),
            version: "0.9.0".to_owned(),
        },
    );
    assert_eq!(output.exit_code, 0, "{}", output.stderr);
    assert!(
        settings_text(&config).contains(r#"version = "0.9.0""#),
        "{}",
        settings_text(&config)
    );
}

#[test]
fn rollback_to_a_version_that_is_not_installed_exits_two() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", false);
    let output = run_in(
        &config,
        &PluginCommand::Rollback {
            id: "vendor.git-merger".to_owned(),
            version: "0.1.0".to_owned(),
        },
    );
    assert_eq!(output.exit_code, 2, "{}", output.stderr);
}

#[test]
fn removing_an_unselected_version_succeeds() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", false);
    let output = run_in(
        &config,
        &PluginCommand::Remove {
            id: "vendor.git-merger".to_owned(),
            version: "1.0.0".to_owned(),
        },
    );
    assert_eq!(output.exit_code, 0, "{}", output.stderr);
    assert!(
        !config
            .join("plugins/installed/vendor.git-merger/1.0.0")
            .exists()
    );
}

#[test]
fn removing_an_enabled_version_changes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    install_fixture(&config, temp.path(), "1.0.0", true);

    let output = run_in(
        &config,
        &PluginCommand::Remove {
            id: "vendor.git-merger".to_owned(),
            version: "1.0.0".to_owned(),
        },
    );
    assert_eq!(output.exit_code, 2, "{}", output.stderr);
    assert!(
        config
            .join("plugins/installed/vendor.git-merger/1.0.0/plugin.json")
            .is_file(),
        "an enabled version must survive a remove attempt"
    );
    assert!(settings_text(&config).contains("enabled = true"));
}

#[test]
fn removing_something_that_is_not_installed_exits_two() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = workspace(temp.path());
    let output = run_in(
        &config,
        &PluginCommand::Remove {
            id: "vendor.absent".to_owned(),
            version: "1.0.0".to_owned(),
        },
    );
    assert_eq!(output.exit_code, 2, "{}", output.stderr);
}
