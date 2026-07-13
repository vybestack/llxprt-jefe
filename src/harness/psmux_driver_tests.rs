use super::*;

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
fn every_driver_gets_a_unique_owned_namespace() {
    let first = TmuxDriver::new();
    let second = TmuxDriver::new();
    assert_ne!(first.namespace, second.namespace);
    assert!(first.diagnostics().contains("namespace: jefe-harness-"));
}

#[test]
fn qualified_psmux_version_is_parsed() {
    assert_eq!(PsmuxVersion::parse("tmux 3.3.6"), Ok(MINIMUM_PSMUX_VERSION));
    assert!(PsmuxVersion::parse("psmux unknown").is_err());
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
