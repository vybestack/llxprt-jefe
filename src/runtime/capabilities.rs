//! Runtime-specific capability detection.
//!
//! Code Puppy currently has no stable, machine-readable model enumeration
//! command. Model entry therefore remains free text behind this boundary until
//! upstream publishes one; do not couple Jefe to Code Puppy's private config.

use std::process::Command;

use crate::domain::{AgentKind, LaunchSignature};

use super::RuntimeError;
use super::commands::{run_command_capture, run_remote_ssh};

/// Model discovery support advertised by an agent runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelDiscovery {
    /// The runtime has no stable model enumeration interface.
    Unavailable,
}

/// Capabilities Jefe can safely use for an agent runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRuntimeCapabilities {
    pub model_discovery: ModelDiscovery,
    pub explicit_yolo: bool,
}

/// Return static capabilities which do not require invoking the runtime.
#[must_use]
pub const fn static_capabilities(kind: AgentKind) -> AgentRuntimeCapabilities {
    match kind {
        AgentKind::Llxprt | AgentKind::CodePuppy => AgentRuntimeCapabilities {
            // Upstream has no stable non-interactive model-list API.
            model_discovery: ModelDiscovery::Unavailable,
            // Code Puppy YOLO support is feature-probed before a configured launch.
            explicit_yolo: false,
        },
    }
}

/// Determine whether help output advertises explicit Code Puppy YOLO values.
#[must_use]
pub fn code_puppy_help_supports_yolo(help: &str) -> bool {
    strip_terminal_controls(help).contains("--yolo")
}

/// Check configured Code Puppy launch options against the selected target.
///
/// `Ok(())` means launch is safe. A diagnostic is returned when the target is
/// too old or cannot be probed; blindly passing an unsupported flag would turn
/// a useful preflight error into a cryptic argparse failure.
pub fn validate_code_puppy_launch(signature: &LaunchSignature) -> Result<(), RuntimeError> {
    if signature.agent_kind != AgentKind::CodePuppy || signature.code_puppy_yolo.is_none() {
        return Ok(());
    }

    let output = if signature.remote.enabled {
        run_remote_ssh(&signature.remote, "code-puppy --help")?
    } else {
        let mut command = Command::new(AgentKind::CodePuppy.binary_name());
        command.arg("--help");
        run_command_capture(command, "code-puppy --help")?
    };

    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    validate_code_puppy_help(output.status.success(), &help)
}

fn validate_code_puppy_help(success: bool, help: &str) -> Result<(), RuntimeError> {
    if success && code_puppy_help_supports_yolo(help) {
        return Ok(());
    }
    Err(RuntimeError::CapabilityCheckFailed(
        "Code Puppy on the launch target does not advertise `--yolo true|false`. Upgrade Code Puppy or edit the agent with a supported version before launching.".to_owned(),
    ))
}

fn strip_terminal_controls(value: &str) -> String {
    let mut plain = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']' | 'P' | 'X' | '^' | '_') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        let _ = chars.next();
                        break;
                    }
                }
            }
            Some(next) => plain.push(next),
            None => {}
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RemoteRepositorySettings, SandboxEngine};
    use std::path::PathBuf;

    fn signature(kind: AgentKind, yolo: Option<bool>) -> LaunchSignature {
        LaunchSignature {
            work_dir: PathBuf::from("/tmp/puppy"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: yolo,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: kind,
        }
    }

    #[test]
    fn detects_yolo_in_plain_and_decorated_help() {
        assert!(code_puppy_help_supports_yolo("--yolo {true,false}"));
        assert!(code_puppy_help_supports_yolo(
            "\u{1b}]11;#000000\u{7}\u{1b}[32m--yolo\u{1b}[0m {true,false}"
        ));
        assert!(!code_puppy_help_supports_yolo("--model MODEL"));
    }

    #[test]
    fn rejects_help_without_explicit_yolo_support() {
        let result = validate_code_puppy_help(true, "--model MODEL");
        assert!(matches!(
            result,
            Err(RuntimeError::CapabilityCheckFailed(_))
        ));
        assert!(validate_code_puppy_help(false, "--yolo {true,false}").is_err());
    }

    #[test]
    fn unknown_escape_sequence_preserves_its_printable_character() {
        assert!(code_puppy_help_supports_yolo("\u{1b}%--yolo {true,false}"));
    }

    #[test]
    fn strips_terminal_string_sequences_and_handles_edge_cases() {
        assert_eq!(strip_terminal_controls(""), "");
        assert_eq!(strip_terminal_controls("\u{1b}[32m\u{1b}[0m"), "");
        assert_eq!(strip_terminal_controls("\u{1b}]title\u{7}--yolo"), "--yolo");
        assert_eq!(
            strip_terminal_controls("\u{1b}Ppayload\u{1b}\\--yolo"),
            "--yolo"
        );
        assert_eq!(
            strip_terminal_controls("before\u{1b}]unterminated"),
            "before"
        );
    }

    #[test]
    fn model_discovery_is_explicitly_unavailable_without_stable_api() {
        assert_eq!(
            static_capabilities(AgentKind::CodePuppy).model_discovery,
            ModelDiscovery::Unavailable
        );
    }

    #[test]
    fn llxprt_and_legacy_code_puppy_do_not_require_yolo_probe() {
        assert!(validate_code_puppy_launch(&signature(AgentKind::Llxprt, Some(true))).is_ok());
        assert!(validate_code_puppy_launch(&signature(AgentKind::CodePuppy, None)).is_ok());
    }
}
