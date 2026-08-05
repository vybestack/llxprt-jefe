//! Behavioral contracts for the platform-aware local multiplexer policy.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver};
use super::agent_launcher::INTERNAL_LAUNCH_ARGUMENT;
use super::multiplexer::{
    LocalPlatform, MultiplexerCapability, MultiplexerError, MultiplexerIdentity,
    MultiplexerIsolation, MultiplexerPlan, MultiplexerVersion, ProbeObservation, classify_probe,
    executable_candidates, validate_namespace,
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
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
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
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.4\n"),
        Ok(MultiplexerVersion::new(3, 4, 0))
    );
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.7b\n"),
        Ok(MultiplexerVersion::new(3, 7, 0))
    );
    for malformed in ["psmux unknown", "tmux 3a.3b.6", "tmux 3..6", "tmux 3.3.6.1"] {
        assert!(matches!(
            MultiplexerVersion::parse(malformed),
            Err(MultiplexerError::MalformedVersion { .. })
        ));
    }
}

#[test]
fn version_parser_accepts_final_release_letter_suffix() {
    // macOS/Homebrew tmux emits `tmux 3.7b`; the trailing release letter must
    // not block session creation preflight (issue #283).
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.7b"),
        Ok(MultiplexerVersion::new(3, 7, 0))
    );
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.3.6a\r\n"),
        Ok(MultiplexerVersion::new(3, 3, 6))
    );
    // The suffix is permitted only on the final present component; everything
    // else stays strict.
    for malformed in [
        "tmux 3a.7",
        "tmux 3.7a.6",
        "tmux 3.7.b",
        "tmux 3.7ab",
        "tmux 3.7B",
        "tmux 3.7-rc1",
        "tmux 3.7.6.1",
    ] {
        assert!(
            matches!(
                MultiplexerVersion::parse(malformed),
                Err(MultiplexerError::MalformedVersion { .. })
            ),
            "version should be rejected: {malformed:?}"
        );
    }
}

#[test]
fn identity_parser_captures_the_psmux_build_commit() {
    // `psmux -V` reports two lines: the tmux compatibility version it emulates,
    // then its own version and the commit it was built from. Keying binary
    // identity on the version alone cannot tell two builds apart, because psmux
    // has shipped many commits under 3.3.7 (issue #547 V10).
    let identity = MultiplexerIdentity::parse("tmux 3.3.7\npsmux 3.3.7 (cb098c0 2026-08-03)\n");

    assert_eq!(
        identity.clone().map(|parsed| parsed.version()),
        Ok(MultiplexerVersion::new(3, 3, 7))
    );
    assert_eq!(
        identity.map(|parsed| parsed.commit().map(str::to_owned)),
        Ok(Some("cb098c0".to_owned()))
    );
}

#[test]
fn identity_parser_reports_no_commit_for_plain_tmux() {
    // Upstream tmux prints only its version, so there is no commit to key on.
    // That absence has to be representable rather than fabricated.
    let identity = MultiplexerIdentity::parse("tmux 3.4\n");

    assert_eq!(
        identity.clone().map(|parsed| parsed.version()),
        Ok(MultiplexerVersion::new(3, 4, 0))
    );
    assert_eq!(
        identity.map(|parsed| parsed.commit().map(str::to_owned)),
        Ok(None)
    );
}

#[test]
fn identity_distinguishes_builds_that_share_a_version() {
    // The whole reason for carrying the commit: these two report the same
    // version and must still not be treated as the same binary.
    let first = MultiplexerIdentity::parse("tmux 3.3.7\npsmux 3.3.7 (cb098c0 2026-08-03)");
    let second = MultiplexerIdentity::parse("tmux 3.3.7\npsmux 3.3.7 (9f2a1de 2026-08-04)");

    assert_eq!(
        first.clone().map(|parsed| parsed.version()),
        second.clone().map(|parsed| parsed.version())
    );
    assert_ne!(first, second);
}

#[test]
fn identity_ignores_a_commit_field_that_is_not_a_hash() {
    // A malformed or missing hash degrades to "no commit" rather than becoming
    // a namespace input that changes on every launch.
    for output in [
        "tmux 3.3.7\npsmux 3.3.7 (unknown 2026-08-03)",
        "tmux 3.3.7\npsmux 3.3.7 ()",
        "tmux 3.3.7\npsmux 3.3.7",
    ] {
        assert_eq!(
            MultiplexerIdentity::parse(output).map(|parsed| parsed.commit().map(str::to_owned)),
            Ok(None),
            "commit should be absent: {output:?}"
        );
    }
}

#[test]
fn probe_classification_accepts_homebrew_tmux_release_letter() {
    let observation = ProbeObservation::Output {
        platform: LocalPlatform::Unix,
        path: PathBuf::from("/opt/homebrew/bin/tmux"),
        status_success: true,
        stdout: "tmux 3.7b\n".to_owned(),
        stderr: String::new(),
    };
    assert_eq!(
        classify_probe(observation).map(|identity| identity.version()),
        Ok(MultiplexerVersion::new(3, 7, 0))
    );
}

#[test]
fn probe_classification_distinguishes_required_failure_modes() {
    let path = PathBuf::from("C:/Program Files/psmux/psmux.exe");
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
            stdout: "tmux 3.3.6".to_owned(),
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
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
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

    let malicious = plan.pane_command_args(
        OsStr::new("llxprt.exe"),
        &[],
        &[(
            OsString::from("SAFE; Write-Error owned"),
            OsString::from("value"),
        )],
    );
    assert!(matches!(
        malicious,
        Err(MultiplexerError::InvalidEnvironmentVariable { .. })
    ));

    let unix = MultiplexerPlan::for_platform(
        LocalPlatform::Unix,
        PathBuf::from("/usr/bin/tmux"),
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe.sock")),
    )
    .unwrap_or_else(|error| panic!("Unix plan should be valid: {error}"));
    assert!(matches!(
        unix.pane_command_args(
            OsStr::new("llxprt"),
            &[],
            &[(OsString::from("SAFE; owned"), OsString::from("value"))],
        ),
        Err(MultiplexerError::InvalidEnvironmentVariable { .. })
    ));
}

#[test]
fn production_namespace_is_stable_while_test_namespaces_are_distinct() {
    if !cfg!(windows) {
        return;
    }
    let production_first = match MultiplexerPlan::current() {
        Ok(plan) => plan,
        Err(_) if std::env::var("JEFE_REQUIRE_PSMUX").as_deref() != Ok("1") => return,
        Err(error) => panic!("required production plan should resolve: {error}"),
    };
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
    let executable = PathBuf::from("C:/Program Files/psmux Ω/psmux.exe");
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable.clone(),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("unicode executable path should be valid: {error}"));
    assert_eq!(plan.executable().as_os_str(), executable.as_os_str());
    assert!(plan.base_args().iter().all(|arg| arg != OsStr::new("-S")));
}

fn resolved_fixture(
    platform: AgentExecutablePlatform,
) -> (
    tempfile::TempDir,
    super::agent_executable::ResolvedAgentExecutable,
) {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("runtime fixture directory should exist: {error}"));
    let binary = if platform == AgentExecutablePlatform::Windows {
        "code-puppy.exe"
    } else {
        "code-puppy"
    };
    let binary_path = directory.path().join(binary);
    std::fs::write(&binary_path, b"fixture")
        .unwrap_or_else(|error| panic!("runtime fixture should be written: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("runtime fixture should be executable: {error}"));
    }
    let executable =
        AgentExecutableResolver::for_platform(platform, vec![directory.path().to_path_buf()], None)
            .resolve("code-puppy")
            .unwrap_or_else(|error| panic!("runtime fixture should resolve: {error}"));
    (directory, executable)
}

#[test]
fn windows_agent_pane_command_uses_staged_session_host_when_provided() {
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));
    let (_directory, executable) = resolved_fixture(AgentExecutablePlatform::Windows);
    let staged_host =
        PathBuf::from("C:/State/session-hosts/jefe-agent-1/<digest>/jefe-session-host.exe");
    let pane = plan
        .agent_pane_command_args_with_staged_host(
            &crate::runtime::AgentPaneLaunch {
                executable: (executable.path(), executable.wrapper_kind()),
                args: &[OsString::from("--profile"), OsString::from("default")],
                environment: &[(OsString::from("LLXPRT_DEBUG"), OsString::from("api"))],
                cwd: Path::new("C:/Repos/my-agent"),
                worker_report: None,
            },
            &staged_host,
        )
        .unwrap_or_else(|error| panic!("staged pane command should build: {error}"));
    assert_eq!(pane.len(), 1);
    let line = pane[0].to_string_lossy();
    assert!(
        line.contains("& 'C:/State/session-hosts/jefe-agent-1/<digest>/jefe-session-host.exe'")
    );
    assert!(line.contains(INTERNAL_LAUNCH_ARGUMENT));
    assert!(!line.contains("build"));
    assert!(!line.contains("'--profile'"));
    assert!(!line.contains("$env:LLXPRT_DEBUG='api'"));
}

#[test]
fn unix_agent_pane_command_has_no_staged_host_path() {
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Unix,
        PathBuf::from("/usr/bin/tmux"),
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe.sock")),
    )
    .unwrap_or_else(|error| panic!("unix plan should be valid: {error}"));
    let (_directory, executable) = resolved_fixture(AgentExecutablePlatform::Unix);
    let staged = PathBuf::from("/state/session-hosts/jefe-agent/host/jefe-session-host.exe");
    let result = plan.agent_pane_command_args_with_staged_host(
        &crate::runtime::AgentPaneLaunch {
            executable: (executable.path(), executable.wrapper_kind()),
            args: &[OsString::from("--profile")],
            environment: &[],
            cwd: Path::new("/repos/my-agent"),
            worker_report: None,
        },
        &staged,
    );
    assert!(
        matches!(result, Err(MultiplexerError::InvalidIsolation { .. })),
        "Unix must reject the Windows-only staged-host path: {result:?}"
    );
}
// ── Issue #456 regression: scrub inherited psmux session variables ──────
//
// Jefe may itself run from inside a psmux session, in which case it inherits
// `PSMUX_SESSION`/`PSMUX_TARGET_SESSION`. Any native Windows local command
// must scrub these so psmux does not refuse to start with
// `sessions should be nested with care`. `std::process::Command::get_envs`
// reports removed entries as `(key, None)`, which lets the scrub be proven
// deterministically without touching the test process environment.

#[test]
fn windows_command_scrubs_inherited_psmux_session_variables() {
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));

    // `command()` rebuilds a fresh `std::process::Command`; `env_remove` marks
    // the variable as removed regardless of whether it was in the parent env,
    // and `get_envs` surfaces that removal as `(key, None)`.
    let command = plan.command();
    let envs: std::collections::HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();

    for variable in ["PSMUX_SESSION", "PSMUX_TARGET_SESSION"] {
        assert_eq!(
            envs.get(OsStr::new(variable)),
            Some(&None),
            "{variable} must be marked removed on the Windows plan command"
        );
    }
}

#[test]
fn windows_command_preserves_base_args_and_executable_after_scrub() {
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));
    let command = plan.command();

    assert_eq!(
        command.get_program(),
        Path::new("C:/Program Files/psmux/psmux.exe")
    );
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(
        args,
        [
            OsStr::new("-f"),
            OsStr::new("NUL"),
            OsStr::new("-L"),
            OsStr::new("jefe-0123456789abcdef")
        ]
    );
}

#[test]
fn windows_command_retains_non_session_psmux_variables() {
    // PSMUX_CLAUDE_TEAMMATE_MODE is not session routing and PSMUX_CONFIG_FILE
    // is already covered by `-f NUL`; both must be retained (not removed).
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        PathBuf::from("C:/Program Files/psmux/psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-0123456789abcdef".to_owned()),
    )
    .unwrap_or_else(|error| panic!("windows plan should be valid: {error}"));
    let mut command = plan.command();
    command.env("PSMUX_CLAUDE_TEAMMATE_MODE", "1");
    command.env("PSMUX_CONFIG_FILE", "NUL");
    let envs: std::collections::HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();
    assert_eq!(
        envs.get(OsStr::new("PSMUX_CLAUDE_TEAMMATE_MODE")),
        Some(&Some(OsStr::new("1"))),
        "team-mode variable must not be scrubbed"
    );
    assert_eq!(
        envs.get(OsStr::new("PSMUX_CONFIG_FILE")),
        Some(&Some(OsStr::new("NUL"))),
        "config-file variable must not be scrubbed"
    );
}

#[test]
fn unix_command_does_not_scrub_psmux_session_variables() {
    // Unix uses upstream tmux on a private socket; psmux session variables are
    // irrelevant there, so the command must not mark any env removals.
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Unix,
        PathBuf::from("/usr/bin/tmux"),
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe.sock")),
    )
    .unwrap_or_else(|error| panic!("unix plan should be valid: {error}"));
    let command = plan.command();

    assert_eq!(command.get_program(), Path::new("/usr/bin/tmux"));
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(
        args,
        [
            OsStr::new("-f"),
            OsStr::new("/dev/null"),
            OsStr::new("-S"),
            OsStr::new("/tmp/jefe.sock")
        ]
    );
    let removed: Vec<&OsStr> = command
        .get_envs()
        .filter_map(|(key, value)| value.is_none().then_some(key))
        .collect();
    assert!(
        removed.is_empty(),
        "unix plan command must not remove any environment variables, got {removed:?}"
    );
}

#[test]
fn psmux_inherited_session_vars_constant_is_exact_session_routing_set() {
    // Guards against accidental widening of the scrub list.
    assert_eq!(
        super::multiplexer::PSMUX_INHERITED_SESSION_VARS,
        ["PSMUX_SESSION", "PSMUX_TARGET_SESSION"]
    );
}
