//! Tests for the external-terminal launch boundary (issue #222, Slice 3).
//!
//! All tests are structural: they assert on `ExternalTerminalPlan` fields and
//! `to_command()` representation without spawning processes.

use super::*;
use std::path::PathBuf;

fn tmp_work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jefe-ext-term-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn non_existent_path() -> PathBuf {
    std::env::temp_dir().join("jefe-ext-term-does-not-exist-999999")
}

// ── A7: plan construction validation ─────────────────────────────────────

#[test]
fn plan_rejects_non_existent_work_dir() {
    let result = build_external_terminal_plan(&non_existent_path(), DesktopPlatform::Linux);
    assert!(matches!(
        result,
        Err(ExternalTerminalError::InvalidWorkDir(_))
    ));
}

#[test]
fn plan_accepts_valid_work_dir() {
    let dir = tmp_work_dir();
    let result = build_external_terminal_plan(&dir, DesktopPlatform::Macos);
    assert!(result.is_ok());
    let plan = result.unwrap_or_else(|e| panic!("plan should succeed: {e}"));
    assert_eq!(plan.work_dir, dir);
}

// ── A9: macOS structural plan ─────────────────────────────────────────────

#[test]
fn macos_plan_uses_open_terminal_app() {
    let dir = tmp_work_dir();
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Macos)
        .unwrap_or_else(|e| panic!("macos plan: {e}"));
    assert_eq!(plan.program, "open");
    assert!(plan.args.contains(&"-a".to_owned()));
    assert!(plan.args.contains(&"Terminal".to_owned()));
    assert!(plan.args.contains(&dir.to_string_lossy().to_string()));
}

// ── A9: Linux structural plan ─────────────────────────────────────────────

#[test]
fn linux_plan_returns_some_emulator_or_error() {
    let dir = tmp_work_dir();
    let result = build_external_terminal_plan(&dir, DesktopPlatform::Linux);
    // Either a plan is found (at least xterm on CI) or NoTerminalFound.
    match result {
        Ok(plan) => {
            assert!(!plan.program.is_empty());
            assert_eq!(plan.work_dir, dir);
        }
        Err(ExternalTerminalError::NoTerminalFound) => {}
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

// ── A9: Windows structural plan (exercised on any CI host) ────────────────

#[test]
fn windows_plan_prefers_wt_exe_or_falls_back_to_cmd() {
    let dir = tmp_work_dir();
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Windows)
        .unwrap_or_else(|e| panic!("windows plan should always succeed: {e}"));
    // wt.exe is preferred; if not found, cmd fallback. Either is valid.
    assert!(
        plan.program == "wt.exe" || plan.program == "cmd",
        "unexpected program: {}",
        plan.program
    );
    assert_eq!(plan.work_dir, dir);
}

#[test]
fn windows_plan_structural_argv_no_shell_string() {
    let dir = tmp_work_dir();
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Windows)
        .unwrap_or_else(|e| panic!("windows plan: {e}"));
    // Every arg is a standalone structural token — no shell command string.
    for arg in &plan.args {
        assert!(!arg.contains("&&"), "dangerous shell operator in: {arg}");
        assert!(!arg.contains(';'), "dangerous separator in: {arg}");
        assert!(!arg.contains("$(("), "dangerous substitution in: {arg}");
        assert!(!arg.starts_with("cd "), "shell cd command in: {arg}");
    }
}

#[test]
fn windows_wt_plan_uses_separate_argv() {
    let dir = tmp_work_dir();
    if let Ok(plan) = build_external_terminal_plan(&dir, DesktopPlatform::Windows)
        && plan.program == "wt.exe"
    {
        let has_d_flag = plan.args.iter().any(|arg| arg == "-d");
        assert!(has_d_flag, "wt.exe plan must have -d as separate arg");
    }
}

// ── JEFE_TERMINAL override (structural) ───────────────────────────────────

#[test]
fn override_plan_is_structural() {
    let dir = tmp_work_dir();
    let plan = super::plan_from_override("alacritty", &dir, DesktopPlatform::Linux);
    assert_eq!(plan.program, "alacritty");
    assert_eq!(plan.work_dir, dir);
}

#[test]
fn override_plan_macos_wraps_with_open() {
    let dir = tmp_work_dir();
    let plan = super::plan_from_override("iTerm", &dir, DesktopPlatform::Macos);
    assert_eq!(plan.program, "open");
    assert!(plan.args.contains(&"iTerm".to_owned()));
}

// ── A7: tmux env scrub (structural verification) ──────────────────────────

#[test]
fn to_command_builds_without_panicking() {
    let dir = tmp_work_dir();
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Macos)
        .unwrap_or_else(|e| panic!("{e}"));
    let cmd = plan.to_command();
    assert_eq!(cmd.get_program(), "open");
}

#[test]
fn tmux_env_scrub_constants_are_complete() {
    // The scrub list must cover the three tmux client vars that leak Jefe's
    // tmux server identity (#171).
    assert!(super::TMUX_ENV_VARS_TO_SCRUB.contains(&"TMUX"));
    assert!(super::TMUX_ENV_VARS_TO_SCRUB.contains(&"TMUX_PANE"));
    assert!(super::TMUX_ENV_VARS_TO_SCRUB.contains(&"TMUX_TMPDIR"));
}

#[test]
fn plan_work_dir_applied_as_current_dir() {
    let dir = tmp_work_dir();
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Linux)
        .ok()
        .or_else(|| build_external_terminal_plan(&dir, DesktopPlatform::Windows).ok());
    let Some(plan) = plan else {
        return; // no emulator on this host is acceptable
    };
    let cmd = plan.to_command();
    assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new(&dir)));
}

// ── ExternalTerminalError Display ─────────────────────────────────────────

#[test]
fn error_display_is_human_readable() {
    let e = ExternalTerminalError::NoTerminalFound;
    assert!(e.to_string().contains("JEFE_TERMINAL"));
    let e2 = ExternalTerminalError::InvalidWorkDir("/bad".to_owned());
    assert!(e2.to_string().contains("/bad"));
    let e3 = ExternalTerminalError::SpawnFailed("boom".to_owned());
    assert!(e3.to_string().contains("boom"));
}

// ── DesktopPlatform::current ──────────────────────────────────────────────

#[test]
fn desktop_platform_current_returns_a_variant() {
    let p = DesktopPlatform::current();
    assert!(matches!(
        p,
        DesktopPlatform::Macos | DesktopPlatform::Linux | DesktopPlatform::Windows
    ));
}
