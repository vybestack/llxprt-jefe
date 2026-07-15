//! Behavioral tests for npm package probe planning, classification, and local boundaries.

use super::*;
use crate::domain::LlxprtNpmPackageSelector;

fn selector(value: &str) -> LlxprtNpmPackageSelector {
    LlxprtNpmPackageSelector::normalize(value)
        .unwrap_or_else(|| panic!("selector fixture must be nonblank"))
}

#[test]
fn local_probe_plan_is_non_installing_structural_argv() {
    let plan = local_probe_arguments(&selector("nightly"));
    assert_eq!(
        plan,
        vec![
            "view",
            "--json",
            "@vybestack/llxprt-code@nightly",
            "version"
        ]
    );
    assert!(!plan.iter().any(|arg| arg == "install" || arg == "exec"));
    remote_probe_shell_escapes_each_dynamic_argument();
}

fn remote_probe_shell_escapes_each_dynamic_argument() {
    let value = "with space's;$(touch x)`touch y`\nline";
    let script = remote_probe_script(&selector(value));
    assert!(script.contains("command -v npm"));
    assert!(script.contains(&shell_escape_single(&format!(
        "@vybestack/llxprt-code@{value}"
    ))));
    assert!(!script.contains("npm install"));
    assert!(!script.contains("npm exec"));
    error_variants_are_typed_and_actionable();
}

fn error_variants_are_typed_and_actionable() {
    let errors = vec![
        NpmPackageAvailabilityError::NpmMissing {
            target: "host".to_owned(),
            selector: "nightly".to_owned(),
        },
        NpmPackageAvailabilityError::ProbeFailure {
            target: "host".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "spawn".to_owned(),
        },
        NpmPackageAvailabilityError::TransportFailure {
            target: "host".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "auth".to_owned(),
        },
        NpmPackageAvailabilityError::ExecutionFailure {
            target: "host".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "signal".to_owned(),
        },
    ];
    for error in errors {
        let message = error.to_string();
        assert!(message.contains("host"));
        assert!(message.contains("nightly"));
    }
    probe_classifier_covers_success_missing_and_unresolved();
}

fn probe_classifier_covers_success_missing_and_unresolved() {
    let selector = selector("nightly");
    assert!(classify_remote_probe(Some(0), "\"1.2.3\"", "", "host", &selector).is_ok());
    assert!(matches!(
        classify_remote_probe(Some(42), NPM_MISSING_SENTINEL, "", "host", &selector),
        Err(NpmPackageAvailabilityError::NpmMissing { .. })
    ));
    assert!(matches!(
        classify_remote_probe(Some(1), "", "E404 package not found", "host", &selector),
        Err(NpmPackageAvailabilityError::PackageUnresolved { .. })
    ));
    assert!(matches!(
        classify_remote_probe(
            Some(42),
            "not-the-sentinel",
            "npm failed",
            "host",
            &selector
        ),
        Err(NpmPackageAvailabilityError::PackageUnresolved { .. })
    ));
    probe_classifier_distinguishes_execution_and_transport_failures();
    diagnostics_are_nonempty_bounded_and_utf8_safe();
}

fn probe_classifier_distinguishes_execution_and_transport_failures() {
    let selector = selector("nightly");
    assert!(matches!(
        classify_remote_probe(None, "", "", "host", &selector),
        Err(NpmPackageAvailabilityError::ExecutionFailure { .. })
    ));
    assert!(matches!(
        classify_remote_probe(Some(255), "", "permission denied", "host", &selector),
        Err(NpmPackageAvailabilityError::TransportFailure { .. })
    ));
    assert!(matches!(
        classify_probe(Some(255), "", "npm registry", "local", &selector, false),
        Err(NpmPackageAvailabilityError::PackageUnresolved { .. })
    ));
}

fn diagnostics_are_nonempty_bounded_and_utf8_safe() {
    let empty = unresolved_error("host", &selector("nightly"), "");
    let NpmPackageAvailabilityError::PackageUnresolved { diagnostic, .. } = empty else {
        panic!("expected unresolved package error");
    };
    assert_eq!(diagnostic, "no diagnostic was returned");

    let input = format!("{}tail", "界".repeat(MAX_DIAGNOSTIC_BYTES));
    let bounded = bounded_diagnostic(&input);
    assert!(bounded.len() <= MAX_DIAGNOSTIC_BYTES);
    assert!(bounded.is_char_boundary(bounded.len()));
    assert!(!bounded.is_empty());
}

#[cfg(unix)]
fn fixture_resolver(script: &str) -> (tempfile::TempDir, AgentExecutableResolver) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm = directory.path().join("npm");
    std::fs::write(&npm, script).unwrap_or_else(|error| panic!("write npm fixture: {error}"));
    std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm fixture: {error}"));
    let resolver = AgentExecutableResolver::for_platform(
        super::super::agent_executable::AgentExecutablePlatform::Unix,
        vec![directory.path().to_path_buf()],
        None,
    );
    (directory, resolver)
}

#[cfg(unix)]
fn local_boundary_executes_fake_npm_with_exact_selector() {
    let script = "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE\"\nprintf '\"0.9.0\"'\n";
    let (directory, resolver) = fixture_resolver(script);
    let capture = directory.path().join("args");
    let mut command = command_for_executable(
        &resolver
            .resolve_target(AgentExecutableTarget::Npm)
            .unwrap_or_else(|error| panic!("resolve fixture: {error}")),
        &local_probe_arguments(&selector("0.9.0"))
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>(),
    );
    command.env("CAPTURE", &capture);
    let output = run_command_capture_with_timeout(command, Duration::from_secs(10), "fixture")
        .unwrap_or_else(|error| panic!("run fixture: {error}"));
    assert!(classify_local_probe(&output, &selector("0.9.0")).is_ok());
    let args =
        std::fs::read_to_string(capture).unwrap_or_else(|error| panic!("read capture: {error}"));
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        ["view", "--json", "@vybestack/llxprt-code@0.9.0", "version"]
    );
}

#[cfg(unix)]
#[test]
fn local_boundary_classifies_missing_nonzero_and_timeout() {
    local_boundary_executes_fake_npm_with_exact_selector();
    let missing_resolver = AgentExecutableResolver::for_platform(
        super::super::agent_executable::AgentExecutablePlatform::Unix,
        Vec::new(),
        None,
    );
    assert!(matches!(
        require_local_with_resolver(
            &selector("nightly"),
            &missing_resolver,
            Duration::from_secs(1)
        ),
        Err(NpmPackageAvailabilityError::NpmMissing { .. })
    ));

    let (_failed_dir, failed) =
        fixture_resolver("#!/bin/sh\nprintf 'registry unavailable' >&2\nexit 42\n");
    assert!(matches!(
        require_local_with_resolver(&selector("nightly"), &failed, Duration::from_secs(1)),
        Err(NpmPackageAvailabilityError::PackageUnresolved { .. })
    ));

    let (_slow_dir, slow) = fixture_resolver("#!/bin/sh\nsleep 2\n");
    assert!(matches!(
        require_local_with_resolver(&selector("nightly"), &slow, Duration::from_millis(50)),
        Err(NpmPackageAvailabilityError::ProbeFailure { .. })
    ));
}
