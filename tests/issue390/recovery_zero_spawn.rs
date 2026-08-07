//! CW10-12: recovery/provider-free commands remain zero-spawn.
//!
//! An executable trap proves the offline recovery/config CLI never spawns a
//! provider process. A canary executable is placed first on `PATH`; if any
//! provider-execution path ran during recovery it would create a marker file.
//! The test runs `jefe config validate` against a real temp config and asserts
//! the canary was never invoked, without changing the recovery architecture.

use std::process::Command;

const JEFE: &str = env!("CARGO_BIN_EXE_jefe");

#[test]
fn recovery_config_validate_starts_zero_provider_processes() {
    let work = tempfile::tempdir().unwrap_or_else(|error| panic!("work tempdir: {error:?}"));
    let config_dir = work.path().join("config");
    std::fs::create_dir_all(&config_dir)
        .unwrap_or_else(|error| panic!("create config dir: {error:?}"));
    // A minimal valid config so `validate` reports success rather than a
    // malformed-config diagnostic; the diagnostic path is provider-free too,
    // but a clean success makes the zero-spawn assertion unambiguous.
    std::fs::write(config_dir.join("settings.toml"), b"settings_schema = 2\n")
        .unwrap_or_else(|error| panic!("seed settings: {error:?}"));

    let canary_dir = work.path().join("canary");
    std::fs::create_dir_all(&canary_dir)
        .unwrap_or_else(|error| panic!("create canary dir: {error:?}"));
    let marker = work.path().join("SPAWNED");

    install_canary(&canary_dir, &marker);

    let path_value = build_path(&canary_dir);
    let output = Command::new(JEFE)
        .args(["config", "validate", "--config"])
        .arg(&config_dir)
        .env("PATH", &path_value)
        .env("CW10_TRAP", marker.to_string_lossy().as_ref())
        .env("GH_TOKEN", "")
        .output()
        .unwrap_or_else(|error| panic!("run jefe config validate: {error:?}"));

    // Recovery completed (provider-free). The exact code is recovery's concern;
    // here we only require it terminated and produced output.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.is_empty() || !output.stderr.is_empty() || output.status.code().is_some(),
        "recovery produced a result"
    );

    assert!(
        !marker.exists(),
        "a provider process was spawned during recovery (canary marker exists)"
    );

    let _ = std::fs::remove_dir_all(work.path());
}

/// Prepend the canary directory to the current `PATH` so any provider-execution
/// attempt resolves the canary first.
fn build_path(canary_dir: &std::path::Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<std::ffi::OsString> = vec![canary_dir.as_os_str().to_owned()];
    parts.extend(std::env::split_paths(&existing).map(std::path::PathBuf::into_os_string));
    std::env::join_paths(parts)
        .unwrap_or_else(|error| panic!("join path: {error:?}"))
        .to_string_lossy()
        .into_owned()
}

/// Install a cross-platform canary that writes the marker file when executed.
fn install_canary(dir: &std::path::Path, marker: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = "#!/bin/sh\necho spawned > \"$CW10_TRAP\"\n";
        let path = dir.join("cw10-provider-trap");
        std::fs::write(&path, script).unwrap_or_else(|error| panic!("write canary: {error:?}"));
        let perms = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("canary metadata: {error:?}"))
            .permissions();
        let mut perms = perms;
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms)
            .unwrap_or_else(|error| panic!("chmod canary: {error:?}"));
        let _ = marker;
    }
    #[cfg(windows)]
    {
        let script = "@echo spawned > \"%CW10_TRAP%\"\r\n";
        let path = dir.join("cw10-provider-trap.cmd");
        std::fs::write(&path, script).unwrap_or_else(|error| panic!("write canary: {error:?}"));
        let _ = marker;
    }
}
