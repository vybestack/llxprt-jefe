//! Process-free static candidate composition and static failure (CWR1-00,
//! CWR1-02).
//!
//! These tests prove the candidate itself: requiredness classification of
//! every selected provider, the static failures that refuse the whole
//! candidate before anything starts or writes, and the one aggregate that
//! carries the composed declarations. None of them may observe a process, a
//! durable write, or a published global.

use super::support::{
    PackageSpec, build, config_root, host, host_binaries, plugins_root, publish_settings,
    resolve_paths, selected_owner, selection_toml, stage, stage_config,
};
use jefe::domain::action_registry::Availability;
use jefe::persistence::plugin_inventory::PluginInventory;
use jefe::startup_candidate::WorkbenchStaticFailure;
use jefe::startup_selection::{DeclarationKind, NotRequiredReason, ProviderRequirement};

/// A selected persistent provider that owns an active action declaration must
/// reach `ready` before anything may spawn (CWR1-00).
#[test]
fn a_persistent_provider_owning_actions_is_required() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::persistent_actions("vendor.required");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.required", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a required provider must still compose: {error}"));

    match selected_owner(&candidate, "vendor.required").requirement() {
        ProviderRequirement::Required { declarations } => {
            assert_eq!(declarations, &vec![DeclarationKind::Actions]);
        }
        other @ ProviderRequirement::NotRequired { .. } => {
            panic!("a persistent actions owner must be required, got {other:?}")
        }
    }
}

/// A config schema alone is an active declaration the persistent provider
/// must be ready to serve: settings the operator edits own the provider too.
#[test]
fn a_persistent_provider_owning_only_a_config_schema_is_required() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec {
        actions: false,
        config: true,
        ..PackageSpec::persistent_actions("vendor.cfg")
    };
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.cfg", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a config-only provider must still compose: {error}"));

    match selected_owner(&candidate, "vendor.cfg").requirement() {
        ProviderRequirement::Required { declarations } => {
            assert_eq!(declarations, &vec![DeclarationKind::Config]);
        }
        other @ ProviderRequirement::NotRequired { .. } => {
            panic!("a persistent config owner must be required, got {other:?}")
        }
    }
}

/// A one-shot provider executes nothing at startup: it is never required to
/// reach `ready` (CWR1-04 is later slices' trap; classification starts here).
#[test]
fn a_one_shot_provider_owns_no_startup_process() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::one_shot("vendor.once");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.once", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a one-shot owner must compose: {error}"));

    assert_eq!(
        selected_owner(&candidate, "vendor.once").requirement(),
        &ProviderRequirement::NotRequired {
            reason: NotRequiredReason::OneShot
        }
    );
}

/// A persistent provider that owns no active declaration starts nothing:
/// metadata and defaults alone do not make it required (decision 4).
#[test]
fn a_declaration_empty_persistent_provider_owns_no_startup_process() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec {
        actions: false,
        ..PackageSpec::persistent_actions("vendor.quiet")
    };
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.quiet", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a declaration-empty owner must compose: {error}"));

    assert_eq!(
        selected_owner(&candidate, "vendor.quiet").requirement(),
        &ProviderRequirement::NotRequired {
            reason: NotRequiredReason::DeclarationEmpty
        }
    );
}

/// A package with no provider at all can never require a process.
#[test]
fn a_package_without_a_provider_is_never_required() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec {
        mode: "none",
        actions: false,
        ..PackageSpec::persistent_actions("vendor.inert")
    };
    let inventory = stage_config(temp.path(), &[(&spec, "{}")]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.inert", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("a provider-free owner must compose: {error}"));

    assert_eq!(
        selected_owner(&candidate, "vendor.inert").requirement(),
        &ProviderRequirement::NotRequired {
            reason: NotRequiredReason::NoProvider
        }
    );
}

/// An active selected configuration that does not validate against the
/// package's schema refuses the candidate before anything is published
/// (CWR1-02).
#[test]
fn an_invalid_selected_configuration_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec {
        actions: false,
        config: true,
        ..PackageSpec::persistent_actions("vendor.badcfg")
    };
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(
        &inventory,
        "settings_schema = 2\n\n[plugins.\"vendor.badcfg\"]\nenabled = true\n\n[plugins.\"vendor.badcfg\".config]\nmode = 42\n",
    );

    match build(&paths, &inventory, &settings, temp.path()) {
        Err(WorkbenchStaticFailure::Provider(refused)) => {
            let rendered = refused.to_string();
            assert!(
                rendered.contains("vendor.badcfg"),
                "the refusal must name the owner, got: {rendered}"
            );
        }
        other => panic!("invalid selected config must refuse, got: {other:?}"),
    }
}

/// A required provider with no Ready binary for this host is a fatal static
/// failure, not an unavailable action: the active declarations it owns have
/// no one to serve them.
#[test]
fn a_required_provider_without_a_host_binary_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::persistent_actions("vendor.nobin");
    let inventory = stage_config(temp.path(), &[(&spec, &super::support::alien_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.nobin", None));

    match build(&paths, &inventory, &settings, temp.path()) {
        Err(WorkbenchStaticFailure::Provider(refused)) => {
            let rendered = refused.to_string();
            assert!(
                rendered.contains("vendor.nobin"),
                "the refusal must name the owner, got: {rendered}"
            );
            assert!(
                rendered.contains(host().as_str()),
                "the refusal must name the host that lacks a binary, got: {rendered}"
            );
        }
        other => panic!("a required provider without a host binary must refuse, got {other:?}"),
    }
}

/// An enabled screen definition that cannot lower refuses the whole
/// candidate: the operator asked for that screen, so a registry without it
/// would be a different workbench (CWR1-02).
#[test]
fn an_enabled_screen_definition_that_cannot_lower_refuses_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = config_root(temp.path());
    let definitions = config.join("definitions");
    std::fs::create_dir_all(&definitions)
        .unwrap_or_else(|error| panic!("definitions must stage: {error}"));
    std::fs::write(definitions.join("bad.screen.toml"), b"not a screen =")
        .unwrap_or_else(|error| panic!("definition must write: {error}"));
    let inventory = PluginInventory::default();
    let paths = resolve_paths(&config);
    let settings = publish_settings(
        &inventory,
        "settings_schema = 2\n\n[workbench]\nenabled_screens = [\"local.bad\"]\n",
    );

    match build(&paths, &inventory, &settings, temp.path()) {
        Err(WorkbenchStaticFailure::Screens(_)) => {}
        other => panic!("a refused enabled definition must refuse, got {other:?}"),
    }
}

/// Candidate construction is process-free and write-free: durable bytes are
/// preserved and no containment directory is created (CWR1-02 side effects).
#[test]
fn candidate_construction_writes_nothing_and_creates_no_containment() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::one_shot("vendor.clean");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let settings_bytes = selection_toml("vendor.clean", None);
    let config = config_root(temp.path());
    std::fs::write(config.join("settings.toml"), settings_bytes.as_bytes())
        .unwrap_or_else(|error| panic!("settings must seed: {error}"));
    let paths = resolve_paths(&config);
    let settings = publish_settings(&inventory, &settings_bytes);

    let result = build(&paths, &inventory, &settings, temp.path());
    assert!(result.is_ok(), "the happy path must compose: {result:?}");

    let retained =
        std::fs::read(config.join("settings.toml")).unwrap_or_else(|e| panic!("read: {e}"));
    assert_eq!(retained, settings_bytes.as_bytes());
    assert!(
        !config.join("state.json").exists(),
        "the candidate must not create durable state"
    );
    for directory in ["home", "tmp", "work"] {
        assert!(
            !temp.path().join(directory).exists(),
            "the candidate must not create the {directory} containment directory"
        );
    }
}

/// The candidate composes one registry with the provider's actions available,
/// retains the one inventory, and carries the shipped agents and screens.
#[test]
fn the_candidate_composes_one_registry_with_provider_actions_and_the_inventory() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let spec = PackageSpec::one_shot("vendor.composed");
    let inventory = stage_config(temp.path(), &[(&spec, &host_binaries())]);
    let paths = resolve_paths(&config_root(temp.path()));
    let settings = publish_settings(&inventory, &selection_toml("vendor.composed", None));

    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("composition must succeed: {error}"));

    let action = jefe::domain::action_registry::ActionId::parse("vendor.composed.run")
        .unwrap_or_else(|error| panic!("action id must parse: {error:?}"));
    assert_eq!(
        candidate.actions().availability_of(&action),
        Some(&Availability::Available)
    );
    assert_eq!(candidate.inventory().packages().len(), 1);
    assert!(!candidate.agent_registry().is_empty());
    assert!(!candidate.screen_registry().screens().is_empty());
    assert_eq!(candidate.screen_warnings().len(), 0);
    assert!(!candidate.providers().catalog().is_empty());
}

/// Normal startup's own published settings — the same shape `main` builds —
/// feed the candidate without translation.
#[test]
fn startup_persistence_feeds_the_candidate() {
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let config = config_root(temp.path());
    let root = plugins_root(&config);
    let spec = PackageSpec::one_shot("vendor.e2e");
    stage(&root, &spec, &host_binaries());
    std::fs::create_dir_all(&config).unwrap_or_else(|error| panic!("config must exist: {error}"));
    std::fs::write(
        config.join("settings.toml"),
        selection_toml("vendor.e2e", Some("1.0.0")).as_bytes(),
    )
    .unwrap_or_else(|error| panic!("settings must seed: {error}"));

    let startup = jefe::startup::build_persistence(Some(&config))
        .unwrap_or_else(|error| panic!("startup must build: {error:?}"));
    let inventory = jefe::startup_candidate::scan_inventory(&startup.paths);

    let candidate = build(&startup.paths, &inventory, &startup.settings, temp.path())
        .unwrap_or_else(|error| panic!("the candidate must compose: {error}"));

    assert_eq!(candidate.selected_owners().len(), 1);
    assert_eq!(
        candidate.settings().plugins.len(),
        1,
        "the aggregate carries the effective settings unchanged"
    );
}
