//! Provider declaration table (issue #389 CW-09, acceptance row D8).

use super::*;

fn triple(value: &str) -> HostTriple {
    HostTriple::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn path(value: &str) -> RelativePath {
    RelativePath::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn binaries(entries: &[(&str, &str)]) -> Vec<(HostTriple, RelativePath)> {
    entries
        .iter()
        .map(|(key, value)| (triple(key), path(value)))
        .collect()
}

#[test]
fn provider_modes_use_lower_kebab_case_wire_names() {
    assert_eq!(ProviderMode::None.as_wire(), "none");
    assert_eq!(ProviderMode::OneShot.as_wire(), "one-shot");
    assert_eq!(ProviderMode::Persistent.as_wire(), "persistent");
    for mode in ProviderMode::ALL {
        assert_eq!(ProviderMode::from_wire(mode.as_wire()), Some(mode));
    }
}

#[test]
fn an_unknown_or_wrong_case_mode_is_not_a_mode() {
    for text in ["One-Shot", "oneshot", "one_shot", "None", "", "daemon"] {
        assert_eq!(
            ProviderMode::from_wire(text),
            None,
            "{text:?} must not name a mode"
        );
    }
}

#[test]
fn a_provider_of_mode_none_declares_no_binaries() {
    let provider = Provider::parse(ProviderMode::None, Vec::new())
        .unwrap_or_else(|error| panic!("mode none must parse: {error}"));
    assert_eq!(provider.mode(), ProviderMode::None);
    assert!(provider.binaries().is_empty());
    assert!(!provider.is_executable());
}

#[test]
fn a_provider_of_mode_none_may_not_declare_a_binary() {
    let error = Provider::parse(
        ProviderMode::None,
        binaries(&[("aarch64-apple-darwin", "bin/p")]),
    )
    .err()
    .unwrap_or_else(|| panic!("mode none with a binary must be rejected"));
    assert_eq!(error, ProviderError::NoneDeclaresBinaries);
}

#[test]
fn an_executable_provider_must_declare_at_least_one_binary() {
    for mode in [ProviderMode::OneShot, ProviderMode::Persistent] {
        assert_eq!(
            Provider::parse(mode, Vec::new()).err(),
            Some(ProviderError::ExecutableWithoutBinaries),
            "{mode:?} must declare a binary"
        );
    }
}

#[test]
fn a_duplicate_host_triple_key_is_rejected() {
    let error = Provider::parse(
        ProviderMode::OneShot,
        binaries(&[
            ("aarch64-apple-darwin", "bin/a"),
            ("aarch64-apple-darwin", "bin/b"),
        ]),
    )
    .err()
    .unwrap_or_else(|| panic!("a duplicate triple must be rejected"));
    assert_eq!(
        error,
        ProviderError::DuplicateTriple {
            triple: "aarch64-apple-darwin".to_owned()
        }
    );
}

#[test]
fn a_declared_binary_is_selected_for_its_exact_triple() {
    let provider = Provider::parse(
        ProviderMode::Persistent,
        binaries(&[
            ("aarch64-apple-darwin", "bin/mac"),
            ("x86_64-unknown-linux-gnu", "bin/linux"),
        ]),
    )
    .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert!(provider.is_executable());

    let selected = provider.select(&triple("aarch64-apple-darwin"));
    assert_eq!(
        selected,
        ProviderSelection::Ready(&path("bin/mac")),
        "the exact triple must select its own binary"
    );
}

#[test]
fn a_near_miss_triple_is_unsupported_rather_than_approximated() {
    let provider = Provider::parse(
        ProviderMode::OneShot,
        binaries(&[("x86_64-unknown-linux-gnu", "bin/linux")]),
    )
    .unwrap_or_else(|error| panic!("must parse: {error}"));

    // Same architecture and OS, different libc. Approximating here would run a
    // binary that cannot load.
    assert_eq!(
        provider.select(&triple("x86_64-unknown-linux-musl")),
        ProviderSelection::UnsupportedPlatform
    );
    assert_eq!(
        provider.select(&triple("aarch64-apple-darwin")),
        ProviderSelection::UnsupportedPlatform
    );
}

#[test]
fn a_provider_of_mode_none_never_selects_a_binary() {
    let provider = Provider::parse(ProviderMode::None, Vec::new())
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(
        provider.select(&triple("aarch64-apple-darwin")),
        ProviderSelection::NotDeclared,
        "a provider-free package is not an unsupported platform"
    );
}

#[test]
fn an_unsupported_platform_reports_the_host_it_lacks() {
    let provider = Provider::parse(
        ProviderMode::OneShot,
        binaries(&[("x86_64-unknown-linux-gnu", "bin/linux")]),
    )
    .unwrap_or_else(|error| panic!("must parse: {error}"));
    let host = triple("aarch64-apple-darwin");
    assert_eq!(
        provider.unsupported_message(&host),
        Some("no binary for aarch64-apple-darwin".to_owned())
    );
}

#[test]
fn declared_triples_are_reported_in_a_deterministic_order() {
    let provider = Provider::parse(
        ProviderMode::OneShot,
        binaries(&[
            ("x86_64-unknown-linux-gnu", "bin/linux"),
            ("aarch64-apple-darwin", "bin/mac"),
        ]),
    )
    .unwrap_or_else(|error| panic!("must parse: {error}"));
    let declared: Vec<&str> = provider.binaries().keys().map(HostTriple::as_str).collect();
    assert_eq!(
        declared,
        vec!["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]
    );
}
