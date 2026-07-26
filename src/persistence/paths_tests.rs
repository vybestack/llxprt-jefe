//! Path precedence and physical-identity contract tests.

use std::ffi::OsString;
use std::path::PathBuf;

use super::diagnostic::CfgCode;
use super::paths::{
    ImportDecision, InspectedSource, PathEnvironment, PathProvenance, PathResolutionRequest,
    Platform, SourceValidity, decide_import, import_state_source, physical_identity, resolve_from,
};
use super::state_v2::StateDocument;
use super::writer::WriteOutcome;

#[test]
fn explicit_config_isolated_override_ignores_all_path_environment() {
    let environment = PathEnvironment {
        jefe_settings_path: Some(OsString::new()),
        jefe_state_path: Some(OsString::new()),
        jefe_config_dir: Some(OsString::new()),
        jefe_state_dir: Some(OsString::new()),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: Some(PathBuf::from("/isolated/jefe")),
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let Ok(paths) = resolve_from(&request, &environment) else {
        panic!("--config must ignore invalid path environment");
    };
    assert_eq!(
        paths.settings.path,
        PathBuf::from("/isolated/jefe/settings.toml")
    );
    assert_eq!(paths.state.path, PathBuf::from("/isolated/jefe/state.json"));
    assert_eq!(
        paths.definitions,
        PathBuf::from("/isolated/jefe/definitions")
    );
    assert!(paths.settings.sources.is_empty());
    assert!(paths.state.sources.is_empty());
}

#[test]
fn linux_defaults_use_xdg_config_and_state_with_data_state_as_source_only() {
    let environment = PathEnvironment {
        home: Some(OsString::from("/home/alice")),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: None,
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let Ok(paths) = resolve_from(&request, &environment) else {
        panic!("standard Linux roots must resolve");
    };
    assert_eq!(
        paths.settings.path,
        PathBuf::from("/home/alice/.config/jefe/settings.toml")
    );
    assert_eq!(
        paths.state.path,
        PathBuf::from("/home/alice/.local/state/jefe/state.json")
    );
    assert_eq!(paths.state.provenance, PathProvenance::PlatformDefault);
    assert_eq!(paths.state.sources.len(), 1);
    assert_eq!(
        paths.state.sources[0].path,
        PathBuf::from("/home/alice/.local/share/jefe/state.json")
    );
    assert_eq!(
        paths.state.sources[0].provenance,
        PathProvenance::HistoricalLinuxDataState
    );
}

#[test]
fn explicit_file_and_directory_values_win_per_file() {
    let environment = PathEnvironment {
        jefe_settings_path: Some(OsString::from("relative/settings.toml")),
        jefe_state_dir: Some(OsString::from("/var/lib/jefe-state")),
        home: Some(OsString::from("/home/alice")),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: None,
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let Ok(paths) = resolve_from(&request, &environment) else {
        panic!("intentional overrides must resolve");
    };
    assert_eq!(
        paths.settings.path,
        PathBuf::from("/work/relative/settings.toml")
    );
    assert_eq!(
        paths.settings.provenance,
        PathProvenance::SettingsPathEnvironment
    );
    assert_eq!(
        paths.state.path,
        PathBuf::from("/var/lib/jefe-state/state.json")
    );
    assert_eq!(
        paths.state.provenance,
        PathProvenance::StateDirectoryEnvironment
    );
}

#[test]
fn empty_path_environment_fails_cfg_e001_without_fallback() {
    let environment = PathEnvironment {
        jefe_state_dir: Some(OsString::new()),
        home: Some(OsString::from("/home/alice")),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: None,
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let error = resolve_from(&request, &environment)
        .err()
        .unwrap_or_else(|| panic!("empty path variable must fail"));
    assert_eq!(error.diagnostic.code, CfgCode::E001);
}

#[cfg(unix)]
#[test]
fn non_unicode_path_environment_fails_cfg_e001() {
    use std::os::unix::ffi::OsStringExt;

    let environment = PathEnvironment {
        jefe_settings_path: Some(OsString::from_vec(vec![0xff])),
        home: Some(OsString::from("/home/alice")),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: None,
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let error = resolve_from(&request, &environment)
        .err()
        .unwrap_or_else(|| panic!("non-Unicode path variable must fail"));
    assert_eq!(error.diagnostic.code, CfgCode::E001);
}

#[test]
fn missing_nested_path_uses_nearest_existing_ancestor_without_creating_directories() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root must be created");
    };
    let target = root.path().join("missing/nested/state.json");
    let Ok(identity) = physical_identity(&target) else {
        panic!("missing path must have a lexical physical identity");
    };
    let Ok(canonical_root) = root.path().canonicalize() else {
        panic!("temporary root must canonicalize");
    };
    assert_eq!(
        identity.canonical_path(),
        canonical_root.join("missing/nested/state.json")
    );
    assert!(!root.path().join("missing").exists());
}

#[cfg(unix)]
#[test]
fn existing_hardlink_aliases_share_physical_identity() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root must be created");
    };
    let original = root.path().join("state.json");
    let alias = root.path().join("state-alias.json");
    assert!(
        std::fs::write(&original, b"{}").is_ok() && std::fs::hard_link(&original, &alias).is_ok(),
        "hardlink fixture must be created"
    );
    let (Ok(original), Ok(alias)) = (physical_identity(&original), physical_identity(&alias))
    else {
        panic!("existing files must resolve physical identity");
    };
    assert!(original.equivalent(&alias));
}

#[test]
fn import_decision_deduplicates_aliases_and_rejects_distinct_ambiguity() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root must be created");
    };
    let first = root.path().join("first.json");
    let alias = root.path().join("alias.json");
    let second = root.path().join("second.json");
    if std::fs::write(&first, b"{}").is_err()
        || std::fs::hard_link(&first, &alias).is_err()
        || std::fs::write(&second, b"{}").is_err()
    {
        panic!("source fixtures must be created");
    }
    let sources = [
        inspected(&first, SourceValidity::Valid),
        inspected(&alias, SourceValidity::Valid),
    ];
    let Ok(ImportDecision::Import { source }) = decide_import(false, None, &sources) else {
        panic!("physical aliases must produce one import");
    };
    assert_eq!(source, first);

    let ambiguous = [
        inspected(&first, SourceValidity::Valid),
        inspected(&second, SourceValidity::Valid),
    ];
    let error = decide_import(false, None, &ambiguous)
        .err()
        .unwrap_or_else(|| panic!("distinct sources must be ambiguous"));
    assert_eq!(error.exit_code, 3);
}

#[test]
fn import_migrates_atomically_and_retains_physically_distinct_source() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root must be created");
    };
    let source = root.path().join("historical/state.json");
    let target = root.path().join("current/state.json");
    let source_bytes = minimal_schema1_state();
    assert!(
        source
            .parent()
            .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
            && std::fs::write(&source, &source_bytes).is_ok(),
        "source fixture must be created"
    );

    let outcome = import_state_source(&source, &target)
        .unwrap_or_else(|error| panic!("valid source must import: {error:?}"));

    assert!(matches!(
        outcome,
        WriteOutcome::Authoritative { revision: 1, .. }
    ));
    assert_eq!(std::fs::read(&source).unwrap_or_default(), source_bytes);
    let target_bytes = std::fs::read(&target).unwrap_or_default();
    let document = StateDocument::parse(&target_bytes)
        .unwrap_or_else(|diagnostics| panic!("target must be schema 2: {diagnostics:?}"));

    assert_eq!(document.state().revision, 1);
    assert_ne!(target_bytes, source_bytes);
}

#[test]
fn malformed_import_reports_cfg_e103_and_leaves_target_absent() {
    let Ok(root) = tempfile::tempdir() else {
        panic!("temporary root must be created");
    };
    let source = root.path().join("historical/state.json");
    let target = root.path().join("current/state.json");
    assert!(
        source
            .parent()
            .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
            && std::fs::write(&source, b"{not json").is_ok(),
        "malformed source fixture must be created"
    );

    let error = import_state_source(&source, &target)
        .err()
        .unwrap_or_else(|| panic!("malformed source must fail"));

    assert_eq!(error.exit_code(), 2);
    assert_eq!(
        error.diagnostic().map(|item| item.code),
        Some(CfgCode::E103)
    );
    assert!(!target.exists());
    assert_eq!(std::fs::read(&source).unwrap_or_default(), b"{not json");
}

#[test]
fn retained_schema1_backups_are_not_source_candidates() {
    let environment = PathEnvironment {
        home: Some(OsString::from("/home/alice")),
        ..PathEnvironment::default()
    };
    let request = PathResolutionRequest {
        config_dir: None,
        platform: Platform::Linux,
        current_dir: PathBuf::from("/work"),
    };
    let paths = resolve_from(&request, &environment)
        .unwrap_or_else(|error| panic!("Linux paths must resolve: {error:?}"));

    assert_eq!(paths.state.sources.len(), 1);
    assert!(paths.state.sources.iter().all(|candidate| {
        !candidate.path.to_string_lossy().contains(".schema1.")
            && candidate.path.extension().and_then(std::ffi::OsStr::to_str) != Some("bak")
    }));
}

fn minimal_schema1_state() -> Vec<u8> {
    br#"{
  "schema_version": 1,
  "repositories": [],
  "agents": [],
  "selected_repository_index": null,
  "selected_agent_index": null
}"#
    .to_vec()
}

fn inspected(path: &std::path::Path, validity: SourceValidity) -> InspectedSource {
    let Ok(identity) = physical_identity(path) else {
        panic!("fixture identity must resolve");
    };
    InspectedSource::new(path.to_path_buf(), identity, validity)
}
