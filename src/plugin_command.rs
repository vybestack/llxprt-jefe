//! Execution of the provider-free `jefe plugin` commands
//! (issue #389 CW-09, acceptance rows C1–C11).
//!
//! Every command here is static. They scan the package roots, validate
//! manifests, write the installed tree, and patch settings through the lossless
//! writer — and none of them starts a provider process. That is not a
//! convention to remember: this module never touches `std::process`, so the
//! guarantee is structural.
//!
//! Exit codes follow the issue's table, and each one means one thing:
//! `2` the request is invalid or names nothing, `3` the request is ambiguous or
//! conflicts with what is installed, `4` the filesystem refused.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::cli::PluginCommand;
use crate::config_owners::builtin_owner_catalog;
use crate::domain::plugin::{HostTriple, PackageCoordinate, PluginId};
use crate::persistence::migration::migrate_settings;
use crate::persistence::paths::{PathResolutionRequest, Platform, ResolvedPaths, resolve_from};
use crate::persistence::plugin_inventory::{InstalledPackage, PluginInventory, scan};
use crate::persistence::plugin_roots::{PluginRootRequest, candidate_roots};
use crate::persistence::settings_edit::{SettingsCandidate, SettingsEdit};
use crate::persistence::writer::{
    AtomicWrite, BackupPolicy, DraftBytes, ExpectedHash, Freshness, write,
};
use crate::recovery::RecoveryOutput;

/// Exit code for a request that is invalid or names nothing installed.
const EXIT_INVALID: u8 = 2;

/// Exit code for a request that is ambiguous or conflicts with what exists.
const EXIT_CONFLICT: u8 = 3;

/// Exit code for a request the filesystem refused.
const EXIT_FILESYSTEM: u8 = 4;

/// Exit code for a malformed invocation, matching `sysexits.h` `EX_USAGE`.
const EXIT_USAGE: u8 = 64;

/// Run one provider-free plugin command.
#[must_use]
pub fn run(command: &PluginCommand, config_dir: Option<&Path>) -> RecoveryOutput {
    let paths = match resolve(config_dir) {
        Ok(paths) => paths,
        Err(output) => return output,
    };
    let inventory = scan(&candidate_roots(&PluginRootRequest {
        executable_dir: std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
        platform: Platform::current(),
        config_plugins_dir: paths.plugins.clone(),
    }));
    match command {
        PluginCommand::List => list(&inventory),
        PluginCommand::Inspect { id, version } => inspect(&inventory, id, version.as_deref()),
        PluginCommand::Install {
            source,
            developer,
            enable,
        } => install(&paths, source, *developer, *enable),
        PluginCommand::Enable { id, version } => {
            enable_package(&paths, &inventory, id, version.as_deref())
        }
        PluginCommand::Disable { id } => disable_package(&paths, id),
        PluginCommand::Rollback { id, version } => {
            enable_package(&paths, &inventory, id, Some(version))
        }
        PluginCommand::Remove { id, version } => remove(&paths, &inventory, id, version),
    }
}

/// Resolve persistence paths without starting any service.
fn resolve(config_dir: Option<&Path>) -> Result<ResolvedPaths, RecoveryOutput> {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let request = PathResolutionRequest {
        config_dir: config_dir.map(Path::to_path_buf),
        platform: Platform::current(),
        current_dir,
    };
    resolve_from(
        &request,
        &crate::persistence::paths::PathEnvironment::capture(),
    )
    .map_err(|error| RecoveryOutput {
        stdout: String::new(),
        stderr: error.diagnostic.redacted_detail.clone(),
        exit_code: error.exit_code,
    })
}

/// A plain success carrying rendered text.
fn ok(stdout: String) -> RecoveryOutput {
    RecoveryOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

/// A failure carrying one operator-facing line.
fn fail(message: String, exit_code: u8) -> RecoveryOutput {
    RecoveryOutput {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit_code,
    }
}

/// `jefe plugin list` — every installed version in listing order.
fn list(inventory: &PluginInventory) -> RecoveryOutput {
    let host = HostTriple::current();
    let mut rendered = String::new();
    for package in inventory.packages() {
        rendered.push_str(&render_row(package, &host));
        rendered.push('\n');
    }
    for ambiguity in inventory.ambiguities() {
        let _ = writeln!(
            rendered,
            "{}  {} {}",
            ambiguity.coordinate(),
            ambiguity.code(),
            ambiguity.code().summary()
        );
    }
    for unavailable in inventory.unavailable() {
        let _ = writeln!(
            rendered,
            "{}  unavailable: {}",
            unavailable.coordinate(),
            unavailable.reason().message()
        );
    }
    ok(rendered)
}

/// One list row: identity, name, and why it cannot run here if so.
fn render_row(package: &InstalledPackage, host: &HostTriple) -> String {
    let mut row = format!("{}  {}", package.coordinate(), package.display_name());
    if let Some(reason) = package.unsupported_reason(host) {
        let _ = write!(row, "  Unsupported platform: {reason}");
    }
    row
}

/// `jefe plugin inspect` — one package, optionally at an exact version.
fn inspect(inventory: &PluginInventory, id: &str, version: Option<&str>) -> RecoveryOutput {
    let Ok(plugin) = PluginId::parse(id) else {
        return fail(format!("{id} is not a plugin id"), EXIT_INVALID);
    };
    if let Some(ambiguity) = inventory
        .ambiguities()
        .iter()
        .find(|entry| entry.coordinate().id() == &plugin)
    {
        return fail(
            format!(
                "{} {}: {}",
                ambiguity.code(),
                ambiguity.coordinate(),
                ambiguity.code().summary()
            ),
            EXIT_CONFLICT,
        );
    }
    let Some(package) = select(inventory, &plugin, version) else {
        return fail(missing(id, version), EXIT_INVALID);
    };
    let host = HostTriple::current();
    let mut rendered = format!("{}\n", render_row(package, &host));
    let _ = writeln!(rendered, "root: {}", package.root().display());
    for alias in package.aliases() {
        let _ = writeln!(rendered, "alias: {}", alias.display());
    }
    rendered.push_str("provider not started\n");
    ok(rendered)
}

/// The highest-precedence installed package for `plugin`, or the exact one.
fn select<'a>(
    inventory: &'a PluginInventory,
    plugin: &PluginId,
    version: Option<&str>,
) -> Option<&'a InstalledPackage> {
    inventory
        .packages()
        .iter()
        .filter(|package| package.coordinate().id() == plugin)
        .find(|package| {
            version.is_none_or(|wanted| package.coordinate().version().as_str() == wanted)
        })
}

/// The message for a package or version that is not installed.
fn missing(id: &str, version: Option<&str>) -> String {
    version.map_or_else(
        || format!("{id} is not installed"),
        |version| format!("{id} {version} is not installed"),
    )
}

/// `jefe plugin install` — commit an archive or a developer directory.
fn install(paths: &ResolvedPaths, source: &Path, developer: bool, enable: bool) -> RecoveryOutput {
    if source.is_dir() && !developer {
        return fail(
            format!(
                "{} is a directory; use --developer to install an unpacked package",
                source.display()
            ),
            EXIT_USAGE,
        );
    }
    let outcome = if developer {
        crate::persistence::plugin_install::install_developer_directory(&paths.plugins, source)
    } else {
        match std::fs::read(source) {
            Ok(bytes) => {
                crate::persistence::plugin_install::install_archive(&paths.plugins, &bytes)
            }
            Err(error) => {
                return fail(format!("{}: {error}", source.display()), EXIT_FILESYSTEM);
            }
        }
    };
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return fail(error.to_string(), install_exit_code(&error)),
    };
    let mut rendered = format!(
        "installed {} at {}\nsha256 {}\n",
        outcome.coordinate(),
        outcome.destination().display(),
        outcome.digest()
    );
    if !enable {
        rendered.push_str("disabled; run jefe plugin enable to trust it\n");
        return ok(rendered);
    }
    rendered.push_str(&trust_notice());
    match apply_edits(paths, &trust_edits(outcome.coordinate())) {
        Ok(()) => ok(rendered),
        Err(output) => output,
    }
}

/// Map an install failure onto its documented exit code.
fn install_exit_code(error: &crate::persistence::plugin_install::InstallError) -> u8 {
    use crate::persistence::plugin_install::InstallError;
    match error {
        InstallError::Archive(_) => EXIT_INVALID,
        InstallError::DestinationExists { .. } => EXIT_CONFLICT,
        InstallError::Filesystem { .. } | InstallError::IndeterminateCommit { .. } => {
            EXIT_FILESYSTEM
        }
    }
}

/// The trust statement shown whenever a package is granted permission to run.
fn trust_notice() -> String {
    "trusted: the provider will execute unsandboxed as your OS user after restart or invocation\n"
        .to_owned()
}

/// The edits that record trust in one exact version.
fn trust_edits(coordinate: &PackageCoordinate) -> Vec<SettingsEdit> {
    vec![
        SettingsEdit::PluginVersion {
            plugin: coordinate.id().owner_id().clone(),
            version: coordinate.version().clone(),
        },
        SettingsEdit::PluginEnabled {
            plugin: coordinate.id().owner_id().clone(),
            enabled: true,
        },
    ]
}

/// `jefe plugin enable` and `jefe plugin rollback`.
fn enable_package(
    paths: &ResolvedPaths,
    inventory: &PluginInventory,
    id: &str,
    version: Option<&str>,
) -> RecoveryOutput {
    let Ok(plugin) = PluginId::parse(id) else {
        return fail(format!("{id} is not a plugin id"), EXIT_INVALID);
    };
    if inventory
        .ambiguities()
        .iter()
        .any(|entry| entry.coordinate().id() == &plugin)
    {
        return fail(
            format!("{id} is ambiguous; remove one package before trusting it"),
            EXIT_CONFLICT,
        );
    }
    let Some(package) = select(inventory, &plugin, version) else {
        return fail(missing(id, version), EXIT_INVALID);
    };
    match apply_edits(paths, &trust_edits(package.coordinate())) {
        Ok(()) => ok(format!(
            "enabled {}\n{}",
            package.coordinate(),
            trust_notice()
        )),
        Err(output) => output,
    }
}

/// `jefe plugin disable` — withdraw trust, preserving the selection.
fn disable_package(paths: &ResolvedPaths, id: &str) -> RecoveryOutput {
    let Ok(plugin) = PluginId::parse(id) else {
        return fail(format!("{id} is not a plugin id"), EXIT_INVALID);
    };
    let edits = [SettingsEdit::PluginEnabled {
        plugin: plugin.owner_id().clone(),
        enabled: false,
    }];
    match apply_edits(paths, &edits) {
        Ok(()) => ok(format!(
            "disabled {id}; its selected version and configuration are preserved\n"
        )),
        Err(output) => output,
    }
}

/// `jefe plugin remove` — delete an installed exact version.
fn remove(
    paths: &ResolvedPaths,
    inventory: &PluginInventory,
    id: &str,
    version: &str,
) -> RecoveryOutput {
    let Ok(coordinate) = PackageCoordinate::parse(id, version) else {
        return fail(format!("{id} {version} is not a package"), EXIT_INVALID);
    };
    let Some(package) = inventory
        .packages()
        .iter()
        .find(|package| package.coordinate() == &coordinate)
    else {
        return fail(missing(id, Some(version)), EXIT_INVALID);
    };
    if is_selected_and_enabled(paths, &coordinate) {
        return fail(
            format!("{coordinate} is enabled; disable it before removing it"),
            EXIT_INVALID,
        );
    }
    match std::fs::remove_dir_all(package.directory()) {
        Ok(()) => ok(format!("removed {coordinate}\n")),
        Err(error) => fail(
            format!("{}: {error}", package.directory().display()),
            EXIT_FILESYSTEM,
        ),
    }
}

/// Whether settings currently trust this exact version.
///
/// Removing a version that is selected and enabled would leave the next start
/// pointing at a package that is gone, so it is refused rather than allowed to
/// break the session later.
fn is_selected_and_enabled(paths: &ResolvedPaths, coordinate: &PackageCoordinate) -> bool {
    let Ok(bytes) = std::fs::read(&paths.settings.path) else {
        return false;
    };
    let Ok(catalog) = builtin_owner_catalog() else {
        return false;
    };
    let Ok(migration) = migrate_settings(&bytes, &catalog) else {
        return false;
    };
    let text = String::from_utf8_lossy(migration.document().original_bytes()).into_owned();
    let owner = format!("[plugins.{:?}]", coordinate.id().as_str());
    let Some(section) = text.split(&owner).nth(1) else {
        return false;
    };
    let block = section.split("\n[").next().unwrap_or(section);
    block.contains("enabled = true")
        && block.contains(&format!("version = {:?}", coordinate.version().as_str()))
}

/// Apply sparse settings edits through the lossless writer.
fn apply_edits(paths: &ResolvedPaths, edits: &[SettingsEdit]) -> Result<(), RecoveryOutput> {
    // An absent settings file is a legitimate starting point; an unreadable
    // one is not. Defaulting both to empty would let a permissions failure
    // turn into a write derived from nothing.
    let bytes = match std::fs::read(&paths.settings.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(fail(
                format!("{}: {error}", paths.settings.path.display()),
                EXIT_FILESYSTEM,
            ));
        }
    };
    let catalog = builtin_owner_catalog().map_err(|error| fail(error.to_string(), EXIT_INVALID))?;
    let migration = migrate_settings(&bytes, &catalog)
        .map_err(|diagnostics| diagnostics_output(&diagnostics, EXIT_INVALID))?;
    let candidate = SettingsCandidate::from_edits(
        &migration,
        &catalog,
        edits,
        ExpectedHash::Present(migration.document().sha256()),
    )
    .map_err(|diagnostics| diagnostics_output(&diagnostics, EXIT_INVALID))?;
    let operation = AtomicWrite {
        target: paths.settings.path.clone(),
        draft: DraftBytes::new(candidate.bytes().to_vec()),
        expected: ExpectedHash::Present(migration.document().sha256()),
        revision: 0,
        backup: BackupPolicy::None,
    };
    write(operation, |_| Freshness::Current)
        .map(|_| ())
        .map_err(|error| fail(error.diagnostic().redacted_detail.clone(), EXIT_FILESYSTEM))
}

/// Render blocking diagnostics as an operator-facing failure.
fn diagnostics_output(
    diagnostics: &[crate::persistence::diagnostic::Diagnostic],
    exit_code: u8,
) -> RecoveryOutput {
    let stderr = diagnostics
        .iter()
        .fold(String::new(), |mut text, diagnostic| {
            let _ = writeln!(text, "{}", diagnostic.redacted_detail);
            text
        });
    RecoveryOutput {
        stdout: String::new(),
        stderr,
        exit_code,
    }
}

#[cfg(test)]
#[path = "plugin_command_tests.rs"]
mod tests;
