//! Runtime preflight checks for sandbox launch ergonomics.

use std::path::Path;
use std::process::Command;

use crate::domain::{PlatformCapabilities, SandboxEngine};

/// A blocking issue detected before launch that requires user action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightIssue {
    /// The container runtime daemon/machine is not running.
    ContainerRuntimeNotRunning {
        engine: SandboxEngine,
        /// Human-readable hint on how to start it (e.g. "podman machine start").
        start_hint: String,
    },
    /// SSH agent is not running or has no identities loaded.
    SshAgentNoIdentities,
    /// The configured sandbox engine is not supported on this platform.
    UnsupportedEngine {
        engine: SandboxEngine,
        platform: &'static str,
        fallback: Option<SandboxEngine>,
    },
}

/// Describes the kind of remediation the user can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightAction {
    /// Run a shell command to start the container runtime.
    StartContainerRuntime {
        engine: SandboxEngine,
        command: String,
    },
    /// Run `ssh-add` to load a key (the user picks the key path interactively).
    SshAdd,
    /// Normalize the engine to a supported runtime.
    SwitchEngine(SandboxEngine),
}

impl PreflightIssue {
    /// Build a user-facing prompt message.
    #[must_use]
    pub fn prompt_message(&self) -> String {
        match self {
            Self::ContainerRuntimeNotRunning { engine, start_hint } => {
                format!(
                    "{} is not running. Start it with `{}`?\n\n\
                     [Enter] start  |  [Esc] cancel launch",
                    engine.label(),
                    start_hint,
                )
            }
            Self::SshAgentNoIdentities => "SSH agent has no identities loaded. Run ssh-add?\n\n\
                 [Enter] run ssh-add  |  [Esc] cancel launch"
                .to_owned(),
            Self::UnsupportedEngine {
                engine,
                platform,
                fallback,
            } => match fallback {
                Some(target) => format!(
                    "{} is not supported on {}. Switch to {}?\n\n\
                     [Enter] switch to {}  |  [Esc] cancel launch",
                    engine.label(),
                    platform,
                    target.label(),
                    target.label(),
                ),
                None => format!(
                    "{} is not supported on {} and no supported sandbox engines are available.\n\n\
                     [Esc] cancel launch",
                    engine.label(),
                    platform,
                ),
            },
        }
    }

    /// Build a user-facing title.
    #[must_use]
    pub fn prompt_title(&self) -> String {
        match self {
            Self::ContainerRuntimeNotRunning { engine, .. } => {
                format!("{} not running", engine.label())
            }
            Self::SshAgentNoIdentities => "SSH agent".to_owned(),
            Self::UnsupportedEngine { engine, .. } => {
                format!("{} unsupported", engine.label())
            }
        }
    }

    /// The remediation action for this issue.
    #[must_use]
    pub fn action(&self) -> PreflightAction {
        match self {
            Self::ContainerRuntimeNotRunning { engine, .. } => {
                let command = match engine {
                    SandboxEngine::Podman => "podman machine start".to_owned(),
                    SandboxEngine::Docker => "open -a Docker".to_owned(),
                    SandboxEngine::Seatbelt => String::new(),
                };
                PreflightAction::StartContainerRuntime {
                    engine: *engine,
                    command,
                }
            }
            Self::SshAgentNoIdentities => PreflightAction::SshAdd,
            Self::UnsupportedEngine {
                fallback: Some(target),
                ..
            } => PreflightAction::SwitchEngine(*target),
            Self::UnsupportedEngine { fallback: None, .. } => {
                PreflightAction::SwitchEngine(SandboxEngine::Podman)
            }
        }
    }
}

/// Check whether the configured container runtime is reachable.
fn container_runtime_is_ready(engine: SandboxEngine) -> bool {
    match engine {
        SandboxEngine::Podman => Command::new("podman")
            .args(["info", "--format", "{{.Host.RemoteSocket.Exists}}"])
            .output()
            .is_ok_and(|out| {
                out.status.success()
                    && String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .eq_ignore_ascii_case("true")
            }),
        SandboxEngine::Docker => {
            // `docker info` exits non-zero when the daemon is unreachable.
            Command::new("docker")
                .args(["info"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }
        SandboxEngine::Seatbelt => true,
    }
}

/// Check whether ssh-agent is running and has at least one identity loaded.
fn ssh_agent_has_identities() -> bool {
    let Ok(sock) = std::env::var("SSH_AUTH_SOCK") else {
        return false;
    };
    if sock.trim().is_empty() || !Path::new(sock.trim()).exists() {
        return false;
    }

    let Ok(output) = Command::new("ssh-add").arg("-l").output() else {
        return false;
    };
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();

    // If the agent reports identities (exit 0 and no "no identities" message),
    // keys are loaded.
    output.status.success() && !combined.contains("the agent has no identities")
}

/// Run all preflight checks for a sandbox-enabled agent launch.
///
/// Returns the first blocking issue found, if any.  The checks are ordered
/// so that the most fundamental problem (unsupported engine, then runtime not
/// running) surfaces first.
#[must_use]
pub fn sandbox_preflight(engine: SandboxEngine) -> Option<PreflightIssue> {
    let caps = PlatformCapabilities::current();
    if !caps.is_engine_supported(engine) {
        return Some(PreflightIssue::UnsupportedEngine {
            engine,
            platform: caps.platform_label(),
            fallback: caps.supported_engines().first().copied(),
        });
    }

    if !container_runtime_is_ready(engine) {
        let start_hint = match engine {
            SandboxEngine::Podman => "podman machine start".to_owned(),
            SandboxEngine::Docker => "open -a Docker".to_owned(),
            SandboxEngine::Seatbelt => return None,
        };
        return Some(PreflightIssue::ContainerRuntimeNotRunning { engine, start_hint });
    }

    if !ssh_agent_has_identities() {
        return Some(PreflightIssue::SshAgentNoIdentities);
    }

    None
}

/// Execute the remediation action. Returns Ok(()) on success or an error message.
pub fn execute_preflight_action(action: &PreflightAction) -> Result<(), String> {
    match action {
        PreflightAction::SwitchEngine(_) => Err(
            "SwitchEngine requires caller state updates; handle this action at the call site."
                .to_owned(),
        ),
        PreflightAction::StartContainerRuntime { command, .. } => {
            if command.is_empty() {
                return Ok(());
            }
            let status = Command::new("sh")
                .arg("-c")
                .arg(command)
                .status()
                .map_err(|e| format!("failed to run `{command}`: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("`{command}` exited with status {status}"))
            }
        }
        PreflightAction::SshAdd => {
            // Find the first private key in ~/.ssh that looks usable.
            let ssh_dir = dirs::home_dir().map(|h| h.join(".ssh")).unwrap_or_default();
            let key_candidates = ["id_ed25519", "id_rsa", "id_ecdsa"];
            let key_path = key_candidates
                .iter()
                .map(|name| ssh_dir.join(name))
                .find(|path| path.exists());

            let Some(key) = key_path else {
                return Err(
                    "no common SSH private key found in ~/.ssh (id_ed25519, id_rsa, id_ecdsa)"
                        .to_owned(),
                );
            };

            let status = Command::new("ssh-add")
                .arg(&key)
                .status()
                .map_err(|e| format!("failed to run ssh-add: {e}"))?;

            if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "ssh-add {} exited with status {status}",
                    key.display()
                ))
            }
        }
    }
}

/// Build a preflight diagnostic summary of supported engines for the current platform.
///
/// Intended for startup logging or debug output, not for blocking the user.
#[must_use]
pub fn platform_engine_diagnostic() -> String {
    let caps = PlatformCapabilities::current();
    let supported: Vec<&str> = caps.supported_engines().iter().map(|e| e.label()).collect();
    format!(
        "Platform: {} | Supported sandbox engines: {}",
        caps.platform_label(),
        supported.join(", ")
    )
}

/// Return a user-facing warning when sandbox SSH forwarding is likely to fail.
///
/// This check is intentionally conservative and non-fatal: it surfaces common
/// host-side issues early (missing/empty SSH agent identities) while still
/// allowing session launch to proceed.
#[must_use]
pub fn sandbox_ssh_agent_warning() -> Option<String> {
    let ssh_auth_sock = std::env::var("SSH_AUTH_SOCK").ok()?;

    if ssh_auth_sock.trim().is_empty() {
        return Some("SSH_AUTH_SOCK is set but empty; SSH auth may fail in sandbox.".to_owned());
    }

    if !Path::new(&ssh_auth_sock).exists() {
        return Some(format!(
            "SSH_AUTH_SOCK points to missing path ({ssh_auth_sock}); SSH auth may fail in sandbox."
        ));
    }

    let output = Command::new("ssh-add").arg("-l").output().ok()?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();

    if combined.contains("the agent has no identities") {
        return Some(
            "SSH agent socket is present but no identities are loaded. Run `ssh-add` (or your key) and relaunch the sandbox session.".to_owned(),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seatbelt_is_always_ready() {
        assert!(container_runtime_is_ready(SandboxEngine::Seatbelt));
    }

    #[test]
    fn seatbelt_sandbox_preflight_returns_none_on_macos() {
        // On macOS, Seatbelt is supported and doesn't require a daemon.
        // On non-macOS, it correctly reports UnsupportedEngine instead.
        let issue = sandbox_preflight(SandboxEngine::Seatbelt);
        if cfg!(target_os = "macos") {
            assert!(
                !matches!(
                    issue,
                    Some(PreflightIssue::ContainerRuntimeNotRunning { .. })
                ),
                "seatbelt should never report container runtime not running on macOS"
            );
            assert!(
                !matches!(issue, Some(PreflightIssue::UnsupportedEngine { .. })),
                "seatbelt should be supported on macOS"
            );
        } else {
            assert!(
                matches!(issue, Some(PreflightIssue::UnsupportedEngine { .. })),
                "seatbelt should report unsupported engine on non-macOS"
            );
        }
    }

    #[test]
    fn preflight_issue_produces_nonempty_prompt() {
        let issue = PreflightIssue::ContainerRuntimeNotRunning {
            engine: SandboxEngine::Podman,
            start_hint: "podman machine start".to_owned(),
        };
        assert!(!issue.prompt_message().is_empty());
        assert!(!issue.prompt_title().is_empty());

        let issue = PreflightIssue::SshAgentNoIdentities;
        assert!(!issue.prompt_message().is_empty());
        assert!(!issue.prompt_title().is_empty());

        let issue = PreflightIssue::UnsupportedEngine {
            engine: SandboxEngine::Seatbelt,
            platform: "Linux",
            fallback: Some(SandboxEngine::Podman),
        };
        assert!(!issue.prompt_message().is_empty());
        assert!(!issue.prompt_title().is_empty());
    }

    #[test]
    fn preflight_action_round_trips() {
        let issue = PreflightIssue::ContainerRuntimeNotRunning {
            engine: SandboxEngine::Docker,
            start_hint: "open -a Docker".to_owned(),
        };
        let action = issue.action();
        assert!(matches!(
            action,
            PreflightAction::StartContainerRuntime {
                engine: SandboxEngine::Docker,
                ..
            }
        ));

        let issue = PreflightIssue::SshAgentNoIdentities;
        assert!(matches!(issue.action(), PreflightAction::SshAdd));

        let issue = PreflightIssue::UnsupportedEngine {
            engine: SandboxEngine::Seatbelt,
            platform: "Linux",
            fallback: Some(SandboxEngine::Podman),
        };
        assert!(matches!(
            issue.action(),
            PreflightAction::SwitchEngine(SandboxEngine::Podman)
        ));
    }

    #[test]
    fn unsupported_engine_prompt_mentions_engine_and_platform() {
        let issue = PreflightIssue::UnsupportedEngine {
            engine: SandboxEngine::Seatbelt,
            platform: "Linux",
            fallback: Some(SandboxEngine::Podman),
        };
        let msg = issue.prompt_message();
        assert!(msg.contains("Seatbelt"), "should mention engine name");
        assert!(msg.contains("Linux"), "should mention platform");
        assert!(msg.contains("Podman"), "should mention fallback");
    }

    #[test]
    fn unsupported_engine_without_fallback_is_cancel_only() {
        let issue = PreflightIssue::UnsupportedEngine {
            engine: SandboxEngine::Seatbelt,
            platform: "Windows",
            fallback: None,
        };
        let msg = issue.prompt_message();
        assert!(msg.contains("no supported sandbox engines are available"));
        assert!(
            !msg.contains("[Enter]"),
            "non-remediable unsupported engine prompts should not advertise Enter remediation"
        );
    }

    #[test]
    fn execute_switch_engine_action_requires_caller_state_handling() {
        let result =
            execute_preflight_action(&PreflightAction::SwitchEngine(SandboxEngine::Podman));
        assert!(result.is_err());
    }

    #[test]
    fn platform_engine_diagnostic_contains_platform_and_engines() {
        let diag = platform_engine_diagnostic();
        assert!(diag.contains("Platform:"), "should contain platform label");
        assert!(
            diag.contains("Supported sandbox engines:"),
            "should contain engine list"
        );

        let supported = PlatformCapabilities::current().supported_engines();
        for engine in supported {
            assert!(diag.contains(engine.label()));
        }

        if supported.is_empty() {
            assert!(
                diag.ends_with("Supported sandbox engines: "),
                "diagnostic should show an empty engine list on unsupported platforms"
            );
        }
    }
}
