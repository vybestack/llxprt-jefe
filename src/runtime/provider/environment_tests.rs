//! Focused tests for provider environment construction and secret redaction
//! (issue #390 CW-10, CW10-14).

use std::collections::BTreeMap;
use std::path::Path;

use super::environment::{
    EnvironmentError, ProviderEnvironment, Redactor, build_process_env, resolve_configure_secrets,
    system_bin_paths,
};
use super::identifiers::EnvName;

struct FixedEnv(&'static [(&'static str, &'static str)]);

impl super::environment::HostEnv for FixedEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

fn env_name(value: &str) -> EnvName {
    EnvName::parse(value).unwrap_or_else(|error| panic!("valid env name: {error:?}"))
}

fn spec(provider_dir: &str) -> ProviderEnvironment {
    ProviderEnvironment {
        provider_dir: Path::new(provider_dir).to_path_buf(),
        ..ProviderEnvironment::default()
    }
}

#[test]
fn environment_begins_with_only_allowed_names() {
    let provider_env = ProviderEnvironment {
        provider_dir: Path::new("/opt/pkg/bin").to_path_buf(),
        ..ProviderEnvironment::default()
    };
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    let names: Vec<&str> = built
        .vars()
        .map(|(key, _)| key.to_str().unwrap_or_else(|| panic!("utf8")))
        .collect();
    assert_eq!(
        names,
        ["HOME", "LANG", "LC_ALL", "PATH", "TMPDIR"],
        "only the five fixed names are present"
    );
}

#[test]
fn path_leads_with_provider_directory_then_system_bins() {
    let built = build_process_env(
        &spec("/opt/pkg/bin"),
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    let path = built
        .get("PATH")
        .unwrap_or_else(|| panic!("PATH present"))
        .to_str()
        .unwrap_or_else(|| panic!("utf8"));
    let mut parts = path.split(':');
    assert_eq!(parts.next(), Some("/opt/pkg/bin"));
    let tail: Vec<&str> = parts.collect();
    let expected: Vec<String> = system_bin_paths()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let expected_refs: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(tail, expected_refs);
}

#[test]
fn nonsecret_declarations_are_present_with_their_values() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .nonsecret
        .insert(env_name("PLUGIN_MODE"), "strict".to_owned());
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C.UTF-8",
        &FixedEnv(&[]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    assert_eq!(
        built.get("PLUGIN_MODE").and_then(|v| v.to_str()),
        Some("strict")
    );
}

#[test]
fn explicit_secret_env_bindings_are_resolved_into_the_process_environment() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .secret_env
        .insert(env_name("API_TOKEN"), env_name("HOST_API_TOKEN"));
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[("HOST_API_TOKEN", "shh-secret-value")]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    assert_eq!(
        built.get("API_TOKEN").and_then(|v| v.to_str()),
        Some("shh-secret-value"),
        "declared secret binding is populated from the host reference"
    );
    assert!(built.has_secrets());
}

#[test]
fn configure_secret_sources_are_not_in_the_process_environment() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[("HOST_DEPLOY_KEY", "super-secret")]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    assert!(
        built.get("DEPLOY_KEY").is_none(),
        "Configure secret values never appear in the process environment"
    );
    assert!(built.has_secrets(), "but they are collected for redaction");
}

#[test]
fn configure_secrets_resolve_the_declared_host_reference() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    let secrets = resolve_configure_secrets(&provider_env, &FixedEnv(&[("HOST_DEPLOY_KEY", "v")]))
        .unwrap_or_else(|error| panic!("resolve: {error:?}"));
    assert_eq!(
        secrets.get(&env_name("DEPLOY_KEY")).map(String::as_str),
        Some("v")
    );
}

#[test]
fn a_missing_declared_secret_source_fails_typed_without_a_value() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .secret_env
        .insert(env_name("API_TOKEN"), env_name("ABSENT_HOST_VAR"));
    let error = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[]),
    )
    .err()
    .unwrap_or_else(|| panic!("missing source must fail"));
    let text = format!("{error}");
    assert!(
        !text.contains("shh") && !text.contains("secret-value"),
        "no secret value reaches the error string: {text}"
    );
    match &error {
        EnvironmentError::UnresolvedSecret { binding, source } => {
            assert_eq!(binding, "API_TOKEN");
            assert_eq!(source, "ABSENT_HOST_VAR");
        }
        EnvironmentError::UndeclaredConfigureSecret { .. } => {
            panic!("expected unresolved secret, not undeclared")
        }
    }
}

#[test]
fn redactor_scrubs_every_known_secret_value() {
    let redactor = Redactor::new(vec!["alpha-secret".to_owned(), "beta".to_owned()]);
    let scrubbed = redactor.redact("log: alpha-secret and beta leaked");
    assert!(
        !scrubbed.contains("alpha-secret") && !scrubbed.contains("beta"),
        "secrets redacted: {scrubbed}"
    );
    assert!(scrubbed.contains(super::environment::REDACTION_PLACEHOLDER));
}

#[test]
fn redactor_is_a_no_op_when_there_are_no_secrets() {
    let redactor = Redactor::new(Vec::new());
    let text = "nothing to hide";
    assert!(matches!(
        redactor.redact(text),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn redactor_replaces_longer_values_first() {
    // A secret that is a prefix of another must not leave a fragment behind.
    let redactor = Redactor::new(vec!["secret".to_owned(), "secret-long".to_owned()]);
    let scrubbed = redactor.redact("secret-long appeared");
    assert!(
        !scrubbed.contains("secret"),
        "the longer secret was scrubbed whole: {scrubbed}"
    );
}

#[test]
fn resolved_secret_count_reports_the_number_of_declarations() {
    let mut provider_env = spec("/opt/pkg/bin");
    provider_env
        .configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    provider_env
        .secret_env
        .insert(env_name("API_TOKEN"), env_name("HOST_API_TOKEN"));
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[("HOST_DEPLOY_KEY", "v1"), ("HOST_API_TOKEN", "v2")]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    // Both a configure-secret and a secret-env declaration were resolved.
    assert!(built.has_secrets());
}

#[test]
fn the_host_environment_is_never_inherited_beyond_declarations() {
    let provider_env = ProviderEnvironment {
        provider_dir: Path::new("/opt/pkg/bin").to_path_buf(),
        nonsecret: BTreeMap::new(),
        secret_env: BTreeMap::new(),
        configure_secret_sources: BTreeMap::new(),
    };
    let built = build_process_env(
        &provider_env,
        Path::new("/tmp/home"),
        Path::new("/tmp/tmp"),
        "C",
        &FixedEnv(&[("HOST_LEAKED", "ambient")]),
    )
    .unwrap_or_else(|error| panic!("build: {error:?}"));
    assert!(
        built.get("HOST_LEAKED").is_none(),
        "an undeclared host variable is never inherited"
    );
}
