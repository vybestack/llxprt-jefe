//! Tests for the external-terminal launch boundary (issue #222, Slice 3).
//!
//! All tests are structural: they assert on `ExternalTerminalPlan` fields and
//! `to_command()` representation without spawning processes.

use super::*;
use std::path::PathBuf;

fn tmp_work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jefe-ext-term-test-{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&dir) {
        panic!("failed to create external-terminal test directory: {error}");
    }
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
fn macos_default_plan_uses_open_terminal_app() {
    // Test the pure default plan directly. The full `build_external_terminal_plan`
    // resolver now honors a detected emulator from the environment (#549), so
    // exercising the default path here keeps the assertion deterministic
    // regardless of which terminal runs `cargo test`.
    let dir = tmp_work_dir();
    let plan = super::plan_macos(&dir);
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

// ── Terminal detection mappings (issue #549) ──────────────────────────────

#[test]
fn macos_maps_iterm_term_program_to_iterm_app() {
    assert_eq!(super::macos_app_for_emulator("iTerm.app"), Some("iTerm"));
}

#[test]
fn macos_maps_iterm_bundle_id_to_iterm_app() {
    assert_eq!(
        super::macos_app_for_emulator("com.googlecode.iterm2"),
        Some("iTerm")
    );
}

#[test]
fn macos_maps_apple_terminal_to_terminal_app() {
    assert_eq!(
        super::macos_app_for_emulator("Apple_Terminal"),
        Some("Terminal")
    );
    assert_eq!(
        super::macos_app_for_emulator("com.apple.Terminal"),
        Some("Terminal")
    );
}

#[test]
fn macos_maps_wezterm_to_wezterm_app() {
    assert_eq!(super::macos_app_for_emulator("WezTerm"), Some("WezTerm"));
}

#[test]
fn macos_unknown_emulator_maps_to_none() {
    assert_eq!(super::macos_app_for_emulator("MysteryTerm"), None);
    assert_eq!(super::macos_app_for_emulator(""), None);
}

#[test]
fn plan_macos_open_is_structural_argv_for_named_app() {
    let dir = tmp_work_dir();
    let plan = super::plan_macos_open(&dir, "iTerm");
    assert_eq!(plan.program, "open");
    assert_eq!(
        plan.args,
        vec![
            "-a".to_owned(),
            "iTerm".to_owned(),
            dir.to_string_lossy().to_string()
        ]
    );
    for arg in &plan.args {
        assert!(!arg.contains("&&"), "dangerous shell operator in: {arg}");
        assert!(!arg.contains(';'), "dangerous separator in: {arg}");
        assert!(!arg.starts_with("cd "), "shell cd command in: {arg}");
    }
}

#[test]
fn plan_from_detected_macos_iterm_produces_open_a_iterm() {
    let dir = tmp_work_dir();
    let Some(plan) = super::plan_from_detected("iTerm.app", &dir, DesktopPlatform::Macos) else {
        panic!("iTerm.app must resolve to a plan");
    };
    assert_eq!(plan.program, "open");
    assert!(plan.args.contains(&"iTerm".to_owned()));
    assert_eq!(plan.work_dir, dir);
}

#[test]
fn plan_from_detected_macos_unknown_falls_back_to_none() {
    let dir = tmp_work_dir();
    assert!(super::plan_from_detected("MysteryTerm", &dir, DesktopPlatform::Macos).is_none());
}

#[test]
fn linux_maps_wezterm_emulator_to_wezterm_plan() {
    let dir = tmp_work_dir();
    let Some(plan) = super::linux_plan_for_emulator("WezTerm", &dir) else {
        panic!("WezTerm must resolve to a plan");
    };
    assert_eq!(plan.program, "wezterm");
    assert!(plan.args.iter().any(|arg| arg == "--cwd"));
    assert!(plan.args.iter().any(|arg| arg == "start"));
    assert_eq!(plan.work_dir, dir);
}

#[test]
fn linux_unknown_emulator_maps_to_none() {
    let dir = tmp_work_dir();
    assert!(super::linux_plan_for_emulator("MysteryTerm", &dir).is_none());
}

#[test]
fn plan_from_detected_windows_is_none_so_default_wins() {
    let dir = tmp_work_dir();
    assert!(
        super::plan_from_detected("Windows Terminal", &dir, DesktopPlatform::Windows).is_none()
    );
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
fn override_plan_macos_runs_arbitrary_executable_directly() {
    let dir = tmp_work_dir();
    let plan = super::plan_from_override("kitty", &dir, DesktopPlatform::Macos);
    assert_eq!(plan.program, "kitty");
    assert!(plan.args.is_empty());
    assert_eq!(plan.work_dir, dir);
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
    let plan = build_external_terminal_plan(&dir, DesktopPlatform::Macos)
        .ok()
        .or_else(|| build_external_terminal_plan(&dir, DesktopPlatform::Linux).ok())
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
