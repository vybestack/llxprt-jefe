use super::*;

/// The harness must not let `JEFE_NAMESPACE` reach the jefe under test (#547).
///
/// Every psmux invocation the harness makes -- including the `new-session` that
/// starts the server whose environment the pane inherits -- goes through
/// `run_owned`, so scrubbing there is what keeps a scenario run inside the
/// namespace its `--config` directory selects rather than the operator's live
/// one.
#[test]
fn the_harness_refuses_to_inherit_a_namespace_override() {
    assert!(
        crate::harness::HARNESS_ENV_VARS_TO_SCRUB.contains(&"JEFE_NAMESPACE"),
        "the namespace override must be scrubbed before psmux spawns jefe"
    );
}

#[test]
fn windows_launch_plan_uses_platform_shell_without_a_unix_wrapper() {
    let request = TmuxStartRequest::command(
        "demo",
        vec![
            "C:\\Program Files\\Jefe Ω\\jefe.exe".to_string(),
            "--config".to_string(),
            "C:\\config dir O'Brien Ω".to_string(),
        ],
        "C:\\working dir Ω",
        100,
        32,
        2_000,
    )
    .unwrap_or_else(|error| panic!("request should be valid: {error}"));

    let args = new_session_args(&request);
    let launch = args.last().map_or("", String::as_str);
    assert_eq!(
        launch,
        "& 'C:\\Program Files\\Jefe Ω\\jefe.exe' '--config' 'C:\\config dir O''Brien Ω'"
    );
    assert!(!launch.contains("unset ") && !launch.contains("exec "));
}

#[test]
fn literal_type_argv_does_not_append_enter() {
    let session = TmuxSession {
        name: "demo".to_string(),
        cols: 100,
        rows: 32,
        keep_session: false,
    };

    assert_eq!(
        literal_send_args(&session, "literal payload"),
        ["send-keys", "-l", "-t", "demo", "--", "literal payload"].map(str::to_string)
    );
}
#[test]
fn every_driver_gets_a_unique_owned_namespace() {
    let first = TmuxDriver::new();
    let second = TmuxDriver::new();
    assert_ne!(first.namespace, second.namespace);
    assert!(first.diagnostics().contains("namespace: jefe-harness-"));
}

#[test]
fn qualified_psmux_version_is_parsed() {
    // The harness shares the runtime's parser so the two cannot drift apart
    // and disagree about which binary they are talking to (issue #547 V10).
    assert_eq!(
        MultiplexerVersion::parse("tmux 3.3.7"),
        Ok(MINIMUM_PSMUX_VERSION)
    );
    assert!(
        MultiplexerVersion::parse("tmux 3.3.6")
            .is_ok_and(|version| version < MINIMUM_PSMUX_VERSION)
    );
    assert!(MultiplexerVersion::parse("psmux unknown").is_err());
}

#[test]
fn harness_qualification_reads_the_same_build_commit_as_the_runtime() {
    // Qualification now yields the full identity, so the harness can tell two
    // psmux builds apart even when they report the same version.
    assert_eq!(
        MultiplexerIdentity::parse(
            "tmux 3.3.7
psmux 3.3.7 (cb098c0 2026-08-03)"
        )
        .map(|identity| identity.commit().map(str::to_owned)),
        Ok(Some("cb098c0".to_owned()))
    );
}

#[test]
fn real_psmux_runs_a_stable_native_process_when_available() {
    let driver = TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let request = TmuxStartRequest::command(
        "driver-real",
        vec![
            "powershell.exe".to_string(),
            "-NoProfile".to_string(),
            "-Command".to_string(),
            "Start-Sleep -Seconds 5".to_string(),
        ],
        std::env::current_dir().unwrap_or_else(|error| panic!("current directory: {error}")),
        100,
        32,
        2_000,
    )
    .unwrap_or_else(|error| panic!("request should be valid: {error}"));
    let session = driver
        .start_session(&request)
        .unwrap_or_else(|error| panic!("psmux session should start: {error}"));
    let capture = driver.capture_screen(&session);
    let cleanup = driver.cleanup_session(&session);
    cleanup.unwrap_or_else(|error| panic!("owned namespace should clean up: {error}"));
    let screen = capture.unwrap_or_else(|error| panic!("screen should capture: {error}"));
    assert_eq!((screen.cols, screen.rows), (100, 32));
}

/// A scenario's launch environment must reach the contained app on Windows
/// too, and the socket override is what keeps a scenario off the operator's
/// live multiplexer server (issue #390).
#[test]
fn the_pane_command_carries_the_requested_environment() {
    let request = TmuxStartRequest::command(
        "env-apply",
        vec!["C:\\jefe.exe".to_string(), "--config".to_string()],
        std::env::temp_dir(),
        80,
        24,
        1000,
    )
    .unwrap_or_else(|error| panic!("request should be valid: {error}"))
    .with_env(vec![(
        "JEFE_SOCKET_PATH".to_string(),
        "C:\\ws\\jefe.sock".to_string(),
    )]);

    let line = windows_command_line(&request.command, &request.env);

    let assignment = line
        .find("$env:JEFE_SOCKET_PATH = 'C:\\ws\\jefe.sock';")
        .unwrap_or_else(|| panic!("requested env missing from pane command: {line}"));
    let invocation = line
        .find("& ")
        .unwrap_or_else(|| panic!("invocation missing from pane command: {line}"));
    assert!(
        assignment < invocation,
        "environment must be assigned before the app is invoked: {line}"
    );
}

/// An environment name reaches the pane command as part of a PowerShell
/// assignment, so a name that is not an identifier must be refused rather than
/// interpolated (issue #390).
#[test]
fn a_hostile_environment_name_is_refused() {
    for hostile in ["x; rm -rf /", "with space", "$(id)", "", "1LEADING"] {
        let request = TmuxStartRequest::command(
            "hostile-env",
            vec!["C:\\jefe.exe".to_string()],
            std::env::temp_dir(),
            80,
            24,
            1000,
        )
        .unwrap_or_else(|error| panic!("request should be valid: {error}"))
        .with_env(vec![(hostile.to_string(), "v".to_string())]);

        assert!(
            request.validate_env().is_err(),
            "env name {hostile:?} must be refused"
        );
    }
}

/// A request that asks for no environment must produce exactly the command it
/// always did, so the change cannot perturb every existing scenario.
#[test]
fn an_env_free_request_is_unchanged() {
    let line = windows_command_line(&["C:\\jefe.exe".to_string()], &[]);
    assert_eq!(line, "& 'C:\\jefe.exe'");
}
