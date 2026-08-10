//! The startup boundary that turns resolved paths and settings into one screen
//! registry (issue #385, CW05-02, CW05-03, CW05-04).

use std::path::PathBuf;

use crate::domain::Id;
use crate::persistence::diagnostic::CfgCode;
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::settings_document::PublishedSettings;
use crate::workbench::ids::ScreenId;

use super::{ScreenStartupError, compose};

/// A definitions directory that removes itself.
struct Config {
    root: PathBuf,
}

impl Config {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "jefe-startup-screens-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("definitions"))
            .unwrap_or_else(|error| unreachable!("fixture config dir must exist: {error}"));
        Self { root }
    }

    fn definition_path(&self, member: &str) -> PathBuf {
        self.root
            .join("definitions")
            .join(format!("{member}.screen.toml"))
    }

    fn write_definition(&self, member: &str, text: &str) {
        std::fs::write(self.definition_path(member), text)
            .unwrap_or_else(|error| unreachable!("fixture definition must be written: {error}"));
    }

    fn paths(&self) -> ResolvedPaths {
        let resolved = |name: &str| ResolvedFile {
            path: self.root.join(name),
            provenance: PathProvenance::ConfigArgument,
            sources: Vec::new(),
        };
        ResolvedPaths {
            settings: resolved("settings.toml"),
            state: resolved("state.json"),
            definitions: self.root.join("definitions"),
            plugins: self.root.join("plugins"),
            themes: self.root.join("themes"),
        }
    }

    fn without_definitions(&self) -> ResolvedPaths {
        let mut paths = self.paths();
        paths.definitions = self.root.join("no-such-directory");
        paths
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn settings_enabling(members: &[&str]) -> PublishedSettings {
    let mut settings = PublishedSettings::default();
    settings.workbench.enabled_screens = members
        .iter()
        .map(|member| {
            Id::parse(&format!("local.{member}"))
                .unwrap_or_else(|error| unreachable!("fixture owner id must parse: {error}"))
        })
        .collect();
    settings
}

/// The definition text the composition tests exercise, with line endings
/// normalized so the variants built below are not silently no-ops on a checkout
/// that converted them.
fn review() -> String {
    include_str!("workbench/testdata/local-review.screen.toml").replace("\r\n", "\n")
}

#[test]
fn a_config_with_no_definitions_directory_composes_the_compiled_screens() {
    let config = Config::new("absent");

    let composition = compose(
        &config.without_definitions(),
        &[],
        &PublishedSettings::default(),
    )
    .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
    assert!(composition.warnings.is_empty());
}

#[test]
fn an_empty_definitions_directory_composes_the_compiled_screens() {
    let config = Config::new("empty");

    let composition = compose(&config.paths(), &[], &PublishedSettings::default())
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
}

#[test]
fn an_enabled_definition_on_disk_joins_the_registry() {
    let config = Config::new("enabled");
    config.write_definition("review", &review());

    let composition = compose(&config.paths(), &[], &settings_enabling(&["review"]))
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(
        composition.registry.screens().len(),
        ScreenId::ALL.len() + 1
    );
    assert!(composition.warnings.is_empty());
}

#[test]
fn a_definition_settings_do_not_enable_is_left_out_without_complaint() {
    let config = Config::new("dormant");
    config.write_definition("review", &review());

    let composition = compose(&config.paths(), &[], &PublishedSettings::default())
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.registry.screens().len(), ScreenId::ALL.len());
    assert!(composition.warnings.is_empty());
}

#[test]
fn an_invalid_dormant_definition_warns_and_keeps_its_bytes() {
    let config = Config::new("dormant-invalid");
    let broken = "screen_schema = 1\nid = \"local.review\"\n";
    config.write_definition("review", broken);

    let composition = compose(&config.paths(), &[], &PublishedSettings::default())
        .unwrap_or_else(|error| unreachable!("composition must publish: {error}"));

    assert_eq!(composition.warnings.len(), 1);
    assert_eq!(composition.warnings[0].code, CfgCode::W004);
    assert_eq!(
        std::fs::read_to_string(config.definition_path("review"))
            .unwrap_or_else(|error| unreachable!("definition must survive: {error}")),
        broken
    );
}

#[test]
fn an_invalid_enabled_definition_refuses_the_whole_registry_and_keeps_its_bytes() {
    let config = Config::new("enabled-invalid");
    let broken = review().replace("type = \"pr-list\"", "type = \"invented-panel\"");
    config.write_definition("review", &broken);

    let outcome = compose(&config.paths(), &[], &settings_enabling(&["review"]));

    assert!(
        matches!(outcome, Err(ScreenStartupError::Refused(_))),
        "an unusable enabled definition must refuse publication"
    );
    assert_eq!(
        std::fs::read_to_string(config.definition_path("review"))
            .unwrap_or_else(|error| unreachable!("definition must survive: {error}")),
        broken
    );
}

#[test]
fn a_refusal_traceable_to_a_file_exits_two() {
    let config = Config::new("exit-codes");
    config.write_definition("review", "not toml {{{");

    let Err(refusal) = compose(&config.paths(), &[], &settings_enabling(&["review"])) else {
        unreachable!("composition must be refused")
    };

    assert_eq!(
        refusal.exit_code(),
        2,
        "a failure traceable to a file on disk is a configuration failure"
    );
}
