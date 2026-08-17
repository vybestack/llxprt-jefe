//! End-to-end startup publication from a real on-disk config (issue #390).
//!
//! These run the exact production path a session takes: resolve and validate
//! persistence from a staged config directory, then publish providers. They
//! exist because the pure composition tests can be green while the wiring that
//! feeds them is not — trust, for instance, is only published for a package the
//! owner catalog already knows about.

use std::fs;
use std::path::Path;

use jefe::domain::action_registry::{ActionId, Availability};
use jefe::domain::plugin::HostTriple;
use jefe::startup_commit::{StartupCommit, commit_startup};

fn manifest(id: &str, triple: &str) -> String {
    format!(
        r#"{{
          "manifest_schema": 1,
          "id": "{id}",
          "version": "1.0.0",
          "display_name": "Vendor Deploy",
          "host_api": {{ "minimum": "0.0.1", "maximum": "99.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "one-shot", "binaries": {{ "{triple}": "bin/deploy" }} }},
          "actions": [
            {{
              "id": "{id}.ship",
              "label": "Ship release",
              "description": "Ship the selected release",
              "category": "{id}",
              "contexts": ["dashboard"],
              "arguments": [],
              "timeout_seconds": 60,
              "destructive": false,
              "confirmation": "none",
              "handler": "ship",
              "allowed_outcomes": ["notice"]
            }}
          ],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#
    )
}

/// Stage a config directory holding one installed package and its trust.
fn stage(root: &Path, id: &str, triple: &str, trusted: bool) {
    let package = root
        .join("plugins")
        .join("installed")
        .join(id)
        .join("1.0.0");
    fs::create_dir_all(package.join("bin")).unwrap_or_else(|e| panic!("stage package: {e}"));
    fs::write(package.join("plugin.json"), manifest(id, triple))
        .unwrap_or_else(|e| panic!("write manifest: {e}"));
    let settings = if trusted {
        format!("settings_schema = 2\n\n[plugins.{id:?}]\nenabled = true\nversion = \"1.0.0\"\n")
    } else {
        "settings_schema = 2\n".to_owned()
    };
    fs::write(root.join("settings.toml"), settings)
        .unwrap_or_else(|e| panic!("write settings: {e}"));
}

fn publish(startup: &mut jefe::startup::StartupPersistence) -> StartupCommit {
    commit_startup(startup).unwrap_or_else(|error| panic!("startup commit must succeed: {error}"))
}

fn action(value: &str) -> ActionId {
    match ActionId::parse(value) {
        Ok(parsed) => parsed,
        Err(error) => panic!("action id must parse: {error}"),
    }
}

/// The whole point of the feature: a trusted package's action must be visible
/// in the one registry every dispatch and every reason string reads from.
#[test]
fn a_trusted_package_publishes_its_action_into_the_session_registry() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(
        &config,
        "vendor.deploy",
        HostTriple::current().as_str(),
        true,
    );

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);
    assert_eq!(
        published.workbench.inventory().packages().len(),
        1,
        "the committed aggregate must retain the startup inventory"
    );

    assert_eq!(
        published
            .workbench
            .actions()
            .availability_of(&action("vendor.deploy.ship")),
        Some(&Availability::Available),
        "a trusted one-shot package must publish its action as available"
    );
    assert!(
        published
            .workbench
            .provider_catalog()
            .get(&action("vendor.deploy.ship"))
            .is_some(),
        "the published action must be invocable"
    );
}

/// The same package, untrusted, must contribute nothing at all.
#[test]
fn an_untrusted_package_publishes_nothing() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(
        &config,
        "vendor.deploy",
        HostTriple::current().as_str(),
        false,
    );

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);

    assert_eq!(
        published
            .workbench
            .actions()
            .availability_of(&action("vendor.deploy.ship")),
        None
    );
}

/// A trusted package with no binary for this host stays visible, but says why.
#[test]
fn an_unsupported_package_publishes_the_reason_the_operator_will_read() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(&config, "vendor.alien", "powerpc64-unknown-linux-gnu", true);

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);

    assert_eq!(
        published
            .workbench
            .actions()
            .availability_of(&action("vendor.alien.ship")),
        Some(&Availability::Unavailable {
            reason: format!("no binary for {}", HostTriple::current().as_str())
        })
    );
    assert!(
        published
            .workbench
            .provider_catalog()
            .get(&action("vendor.alien.ship"))
            .is_none(),
        "an unsupported action must never be invocable"
    );
}

/// CW10-13: the operator must be able to find the action and, when it cannot
/// run, the reason — in the surface they actually open.
#[test]
fn a_published_package_action_is_visible_in_the_help_the_operator_opens() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(
        &config,
        "vendor.deploy",
        HostTriple::current().as_str(),
        true,
    );

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);

    let help = jefe::ui::modals::effective_help_content_lines(published.workbench.actions(), None);

    assert!(
        help.iter().any(|line| line == "Packages:"),
        "Help must carry a package section: {help:?}"
    );
    assert!(
        help.iter().any(|line| line.contains("Ship release")),
        "Help must name the package action: {help:?}"
    );
}

/// The unavailable reason an operator reads in Help must be the snapshot's own
/// bytes, so it cannot disagree with a refused keybind.
#[test]
fn help_quotes_the_snapshot_reason_for_an_unsupported_package() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(&config, "vendor.alien", "powerpc64-unknown-linux-gnu", true);

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);

    let expected = format!("no binary for {}", HostTriple::current().as_str());
    let help = jefe::ui::modals::effective_help_content_lines(published.workbench.actions(), None);
    assert!(
        help.iter().any(|line| line.contains(&expected)),
        "Help must quote the snapshot reason verbatim: {help:?}"
    );
}

/// One-shot providers are invocation-only, so committing their declarations
/// must not prepare process containment during startup.
#[test]
fn one_shot_publication_creates_no_startup_containment() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = temp.path().join("config");
    fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e}"));
    stage(
        &config,
        "vendor.deploy",
        HostTriple::current().as_str(),
        true,
    );

    let mut startup = match jefe::startup::build_persistence(Some(&config)) {
        Ok(value) => value,
        Err(error) => panic!("startup must resolve: {error:?}"),
    };
    let published = publish(&mut startup);

    let action = action("vendor.deploy.ship");
    let Some(descriptor) = published.workbench.provider_catalog().get(&action) else {
        panic!("the action must be published");
    };
    for (label, dir) in [
        ("working_dir", &descriptor.working_dir),
        ("home", &descriptor.home),
        ("tmpdir", &descriptor.tmpdir),
    ] {
        assert!(
            !dir.exists(),
            "{label} {} must remain absent until one-shot invocation",
            dir.display()
        );
    }
}
