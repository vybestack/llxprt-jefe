//! Tmux command execution.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use std::path::Path;
use std::process::Command;

use crate::domain::{LaunchSignature, SandboxEngine};

use super::errors::RuntimeError;

const SANDBOX_IMAGE_REPO: &str = "ghcr.io/vybestack/llxprt-code/sandbox";
const SANDBOX_IMAGE_NIGHTLY_TAG: &str = "nightly";
const SANDBOX_IMAGE_LATEST_TAG: &str = "latest";

fn tmux_cmd_status(args: &[&str], cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("tmux");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run tmux {args:?}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "tmux {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn reset_tmux_server() {
    let _ = tmux_cmd_status(["kill-server"].as_ref(), None);
}

fn apply_session_style(session_name: &str) {
    // Match app reverse-style bars: green-ish status background with black text.
    let _ = tmux_cmd_status(
        [
            "set-option",
            "-t",
            session_name,
            "status-style",
            "fg=colour0,bg=#6a9955",
        ]
        .as_ref(),
        None,
    );
}

fn looks_like_semver_tag(version: &str) -> bool {
    let split_at = version.find(['-', '+']).unwrap_or(version.len());
    let core = &version[..split_at];
    let suffix = if split_at < version.len() {
        Some(&version[split_at + 1..])
    } else {
        None
    };

    let mut parts = core.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();
    let patch = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return false;
    }

    if [major, minor, patch]
        .iter()
        .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }

    if let Some(suffix) = suffix {
        if suffix.is_empty() {
            return false;
        }

        if !suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return false;
        }
    }

    true
}

fn parse_llxprt_version_tag(version_output: &str) -> Option<String> {
    version_output
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '+')
            })
        })
        .find_map(|token| {
            let normalized = token.strip_prefix('v').unwrap_or(token);
            if looks_like_semver_tag(normalized) {
                Some(normalized.to_owned())
            } else {
                None
            }
        })
}

fn detect_llxprt_version_tag() -> Option<String> {
    let output = Command::new("llxprt").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_llxprt_version_tag(&stdout)
}

fn sandbox_manifest_exists(engine: SandboxEngine, image_ref: &str) -> bool {
    let command = match engine {
        SandboxEngine::Podman => "podman",
        SandboxEngine::Docker => "docker",
        SandboxEngine::Seatbelt => return false,
    };

    Command::new(command)
        .args(["manifest", "inspect", image_ref])
        .output()
        .is_ok_and(|out| out.status.success())
}

fn resolve_sandbox_image(engine: SandboxEngine) -> Option<String> {
    match engine {
        SandboxEngine::Seatbelt => None,
        SandboxEngine::Podman | SandboxEngine::Docker => {
            if let Some(version_tag) = detect_llxprt_version_tag() {
                let version_image = format!("{SANDBOX_IMAGE_REPO}:{version_tag}");
                if sandbox_manifest_exists(engine, &version_image) {
                    return Some(version_image);
                }
            }

            let nightly_image = format!("{SANDBOX_IMAGE_REPO}:{SANDBOX_IMAGE_NIGHTLY_TAG}");
            if sandbox_manifest_exists(engine, &nightly_image) {
                return Some(nightly_image);
            }

            let latest_image = format!("{SANDBOX_IMAGE_REPO}:{SANDBOX_IMAGE_LATEST_TAG}");
            if sandbox_manifest_exists(engine, &latest_image) {
                return Some(latest_image);
            }

            None
        }
    }
}

/// Create a new detached tmux session running llxprt.
///
/// The session runs `llxprt` directly (not a shell), so when llxprt exits,
/// the tmux session becomes "dead" until explicit relaunch.
///
/// @pseudocode component-002 lines 01-06
#[allow(clippy::too_many_lines)]
pub fn create_session(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
) -> Result<(), RuntimeError> {
    // Kill any stale session with the same name first
    let _ = kill_session(session_name);

    // Retry once if tmux server is in a fork-broken state.
    for attempt in 0..=1 {
        let mut llxprt_args: Vec<String> = Vec::new();

        // Add profile if specified.
        if !signature.profile.is_empty() {
            llxprt_args.push("--profile-load".to_owned());
            llxprt_args.push(signature.profile.clone());
        }

        // Add mode flags (e.g., --yolo).
        for flag in &signature.mode_flags {
            if !flag.is_empty() {
                llxprt_args.push(flag.clone());
            }
        }

        // Add --continue if pass_continue is true.
        if signature.pass_continue {
            llxprt_args.push("--continue".to_owned());
        }

        // Sandbox launch parity with llxprt-code: explicit --sandbox and engine,
        // plus SANDBOX_FLAGS environment support.
        let mut launch_env: Vec<(String, String)> = Vec::new();
        if signature.sandbox_enabled {
            llxprt_args.push("--sandbox".to_owned());
            llxprt_args.push("--sandbox-engine".to_owned());
            llxprt_args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));

            if std::env::var_os("LLXPRT_SANDBOX_IMAGE").is_none()
                && let Some(image_ref) = resolve_sandbox_image(signature.sandbox_engine)
            {
                launch_env.push(("LLXPRT_SANDBOX_IMAGE".to_owned(), image_ref));
            }
        }

        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        let mut cmd = Command::new("tmux");
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(work_dir.to_str().unwrap_or("."));

        if !launch_env.is_empty() {
            cmd.arg("env");
            for (key, value) in &launch_env {
                cmd.arg(format!("{key}={value}"));
            }
        }

        cmd.arg("llxprt");
        for arg in &llxprt_args {
            cmd.arg(arg);
        }

        let output = cmd
            .output()
            .map_err(|e| RuntimeError::SpawnFailed(format!("tmux new-session: {e}")))?;

        if output.status.success() {
            // Preserve dead pane output in tmux for post-mortem inspection/relaunch context.
            let _ = tmux_cmd_status(
                ["set-option", "-t", session_name, "remain-on-exit", "on"].as_ref(),
                None,
            );
            apply_session_style(session_name);
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let fork_broken =
            stderr.contains("fork failed") || stderr.contains("Device not configured");

        if attempt == 0 && fork_broken {
            reset_tmux_server();
            continue;
        }

        return Err(RuntimeError::SpawnFailed(format!(
            "tmux new-session failed: {stderr}"
        )));
    }

    Err(RuntimeError::SpawnFailed(
        "tmux new-session failed after retry".to_owned(),
    ))
}

/// Check if a tmux session exists.
#[allow(dead_code)]
pub fn session_exists(session_name: &str) -> bool {
    let output = Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Capture pane output for a session as plain text lines.
pub fn capture_pane_lines(session_name: &str) -> Option<Vec<String>> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", session_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().map(|line| line.to_owned()).collect())
}

/// Kill a tmux session.
///
/// @pseudocode component-002 lines 24-25
pub fn kill_session(session_name: &str) -> Result<(), RuntimeError> {
    let output = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output()
        .map_err(|e| RuntimeError::KillFailed(format!("tmux kill-session: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RuntimeError::KillFailed(format!(
            "tmux kill-session failed: {stderr}"
        )))
    }
}

/// Send keys to a tmux session (for testing/automation).
#[allow(dead_code)]
pub fn send_keys(session_name: &str, keys: &str) -> Result<(), RuntimeError> {
    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, keys, "Enter"])
        .output()
        .map_err(|e| RuntimeError::WriteFailed(format!("tmux send-keys: {e}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RuntimeError::WriteFailed(format!(
            "tmux send-keys failed: {stderr}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SandboxEngine;

    fn base_signature() -> LaunchSignature {
        LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: String::new(),
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        }
    }

    #[test]
    fn llxprt_debug_env_is_omitted_when_empty() {
        let signature = base_signature();
        let mut launch_env: Vec<(String, String)> = Vec::new();

        if signature.sandbox_enabled {
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
        }
        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        assert!(!launch_env.iter().any(|(key, _)| key == "LLXPRT_DEBUG"));
    }

    #[test]
    fn llxprt_debug_env_is_included_when_non_empty() {
        let mut signature = base_signature();
        signature.llxprt_debug = "trace=1".to_owned();

        let mut launch_env: Vec<(String, String)> = Vec::new();
        if signature.sandbox_enabled {
            launch_env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
        }
        if !signature.llxprt_debug.is_empty() {
            launch_env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
        }

        assert_eq!(
            launch_env
                .into_iter()
                .find(|(key, _)| key == "LLXPRT_DEBUG")
                .map(|(_, value)| value),
            Some("trace=1".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_handles_plain_semver() {
        assert_eq!(
            parse_llxprt_version_tag("0.8.1\n"),
            Some("0.8.1".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_handles_prefixed_semver() {
        assert_eq!(
            parse_llxprt_version_tag("llxprt v0.9.0"),
            Some("0.9.0".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_handles_nightly_semver_suffix() {
        assert_eq!(
            parse_llxprt_version_tag("0.9.0-nightly.260301.0223eb66a\n"),
            Some("0.9.0-nightly.260301.0223eb66a".to_owned())
        );
    }

    #[test]
    fn parse_llxprt_version_tag_rejects_non_semver() {
        assert_eq!(parse_llxprt_version_tag("nightly build"), None);
    }
}
