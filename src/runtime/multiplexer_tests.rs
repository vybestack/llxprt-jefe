//! Behavioral contracts for the platform-aware local multiplexer policy.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::multiplexer::{
    LocalPlatform, MultiplexerCapability, MultiplexerError, MultiplexerIsolation, MultiplexerPlan,
    MultiplexerVersion, ProbeObservation, classify_probe, executable_candidates,
    validate_namespace,
};

#[test]
fn platform_policy_builds_unix_socket_and_windows_namespace_arguments() {
    let unix = MultiplexerPlan::for_platform(
        LocalPlatform::Unix,
        PathBuf::from("/usr/bin/tmux"),
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe.sock")),
    )
    .unwrap_or_else(|error| panic!("unix plan should be valid: {error}"));
    assert_eq!(unix.executable(), Path::new("/usr/bin/tmux"));
    assert_eq!(
        unix.base_args(),
        ["-f", "/dev/null", "-S", "/tmp/jefe.sock"].map(OsString::from)
    );

    let windows = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from(r"C:\Program Files\psmux\psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));
    assert_eq!(
        windows.base_args(),
        ["-f", "NUL", "-L", "jefe-0123456789abcdef"].map(OsString::from)
    );
    assert!(!windows.base_args().iter().any(|arg| arg == "/dev/null"));
    assert!(!windows.base_args().iter().any(|arg| arg == "-S"));
}

#[test]
fn executable_candidates_never_fall_back_to_compatibility_tmux_on_windows() {
    assert_eq!(
        executable_candidates(LocalPlatform::Windows),
        [OsString::from("psmux.exe"), OsString::from("psmux")]
    );
    assert_eq!(
        executable_candidates(LocalPlatform::Unix),
        [OsString::from("tmux")]
    );
}

#[test]
fn namespace_validation_accepts_private_ascii_and_rejects_unsafe_values() {
    assert!(validate_namespace("jefe-0123456789abcdef").is_ok());
    for invalid in ["", "jefe space", "../jefe", "jefe/other", "jefe_Ω"] {
        assert!(
            matches!(
                validate_namespace(invalid),
                Err(MultiplexerError::InvalidNamespace { .. })
            ),
            "namespace should be rejected: {invalid:?}"
        );
    }
}

#[test]
fn version_parser_accepts_tmux_compatible_psmux_output() {
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.3.6\r\n"),
        Ok(MultiplexerVersion::new(3, 3, 6))
    );
    assert!(matches!(
        MultiplexerVersion::parse("psmux unknown"),
        Err(MultiplexerError::MalformedVersion { .. })
    ));
}

#[test]
fn probe_classification_distinguishes_required_failure_modes() {
    let path = PathBuf::from(r"C:\Program Files\psmux\psmux.exe");
    assert!(matches!(
        classify_probe(ProbeObservation::Missing {
            platform: LocalPlatform::Windows,
            path: path.clone(),
        }),
        Err(MultiplexerError::MissingExecutable { .. })
    ));
    assert!(matches!(
        classify_probe(ProbeObservation::LaunchFailed {
            platform: LocalPlatform::Windows,
            path: path.clone(),
            reason: "denied".to_owned(),
        }),
        Err(MultiplexerError::LaunchFailed { .. })
    ));
    assert!(matches!(
        classify_probe(ProbeObservation::Output {
            platform: LocalPlatform::Windows,
            path: path.clone(),
            status_success: true,
            stdout: "tmux 3.3.5".to_owned(),
            stderr: String::new(),
        }),
        Err(MultiplexerError::UnsupportedVersion { .. })
    ));
    assert!(matches!(
        classify_probe(ProbeObservation::CapabilityMissing {
            platform: LocalPlatform::Windows,
            path,
            version: MultiplexerVersion::new(3, 3, 6),
            capability: MultiplexerCapability::NamespaceIsolation,
        }),
        Err(MultiplexerError::RequiredCapabilityUnavailable { .. })
    ));
}

#[test]
fn windows_rejects_shadowed_compatibility_environment_executables() {
    for path in [
        r"C:\Windows\System32\wsl.exe",
        r"C:\cygwin64\bin\tmux.exe",
        r"C:\Program Files\Git\usr\bin\tmux.exe",
        r"C:\msys64\usr\bin\tmux.exe",
    ] {
        let error = MultiplexerPlan::for_platform(
            LocalPlatform::Windows,
            PathBuf::from(path),
            MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
        );
        assert!(
            matches!(error, Err(MultiplexerError::RejectedExecutable { .. })),
            "compatibility executable must be rejected: {path}"
        );
    }
}

#[test]
fn windows_pane_command_uses_powershell_without_unix_env_wrapper() {
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from(r"C:\Program Files\psmux\psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));
    let pane = plan
        .pane_command_args(
            OsStr::new(r"C:\Program Files\LLxprt Ω\llxprt.exe"),
            &[OsString::from("--profile"), OsString::from("O'Brien")],
            &[(OsString::from("LLXPRT_DEBUG"), OsString::from("api"))],
        )
        .unwrap_or_else(|error| panic!("Windows pane command should build: {error}"));
    assert_eq!(pane.len(), 1);
    let line = pane[0].to_string_lossy();
    assert!(line.contains("$env:TMUX=$null"));
    assert!(line.contains("$env:TMUX_PANE=$null"));
    assert!(line.contains("$env:TMUX_TMPDIR=$null"));
    assert!(line.contains("& 'C:\\Program Files\\LLxprt Ω\\llxprt.exe'"));
    assert!(line.contains("'O''Brien'"));
    assert!(!line.contains("env -u"));
}

#[test]
fn production_namespace_is_stable_while_test_namespaces_are_distinct() {
    if !cfg!(windows) {
        return;
    }
    let production_first = MultiplexerPlan::current()
        .unwrap_or_else(|error| panic!("first production plan should resolve: {error}"));
    let production_second = MultiplexerPlan::current()
        .unwrap_or_else(|error| panic!("second production plan should resolve: {error}"));
    assert_eq!(production_first.isolation(), production_second.isolation());

    let first = MultiplexerPlan::current_for_test()
        .unwrap_or_else(|error| panic!("first test plan should resolve: {error}"));
    let second = MultiplexerPlan::current_for_test()
        .unwrap_or_else(|error| panic!("second test plan should resolve: {error}"));
    assert_ne!(first.isolation(), second.isolation());
}

#[test]
fn guarded_real_multiplexer_preflight_qualifies_the_current_dependency() {
    let plan = match MultiplexerPlan::current_for_test() {
        Ok(plan) => plan,
        Err(_) if std::env::var("JEFE_REQUIRE_PSMUX").as_deref() != Ok("1") => return,
        Err(error) => panic!("required multiplexer should resolve: {error}"),
    };
    let result = plan.preflight(&[
        MultiplexerCapability::AttachSession,
        MultiplexerCapability::PaneCapture,
    ]);
    assert!(
        result.is_ok(),
        "real multiplexer preflight failed: {result:?}"
    );
}

#[test]
fn path_arguments_remain_os_strings_without_lossy_conversion() {
    let executable = PathBuf::from(r"C:\Program Files\psmux Ω\psmux.exe");
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable.clone(),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("unicode executable path should be valid: {error}"));
    assert_eq!(plan.executable().as_os_str(), executable.as_os_str());
    assert!(plan.base_args().iter().all(|arg| arg != OsStr::new("-S")));
}
