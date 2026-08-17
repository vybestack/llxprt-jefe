//! Startup diagnostics remain available when the provider transaction refuses
//! the commit before the TUI can start.

use std::process::Command;

use super::transaction_support::{Scene, process_budget, settings_for};

const JEFE: &str = env!("CARGO_BIN_EXE_jefe");

#[test]
fn provider_transaction_failure_is_written_to_the_configured_log() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_unloadable_binary("required.provider");
    std::fs::write(
        scene.config.join("settings.toml"),
        settings_for(&["required.provider"]),
    )
    .unwrap_or_else(|error| panic!("write settings: {error:?}"));
    let log_path = scene.config.join("startup-failure.log");

    let output = Command::new(JEFE)
        .args(["--config"])
        .arg(&scene.config)
        .env("JEFE_LOG_FILE", &log_path)
        .env("JEFE_LOG", "info")
        .output()
        .unwrap_or_else(|error| panic!("run jefe: {error:?}"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected startup exit; stderr: {stderr}"
    );
    assert!(
        stderr.contains("required provider startup failed"),
        "startup refusal must remain visible on stderr: {stderr}"
    );

    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|error| panic!("read startup diagnostics: {error:?}"));
    assert!(
        log.contains("jefe starting"),
        "logging did not initialize: {log}"
    );
    assert!(
        log.contains("startup commit failed") && log.contains("required provider startup failed"),
        "provider failure was not logged before exit: {log}"
    );
    assert!(
        log.contains("required.provider"),
        "failure log omitted the provider identity: {log}"
    );
}
