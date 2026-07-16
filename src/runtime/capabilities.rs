//! Runtime-specific capability detection.
//!
//! Code Puppy currently has no stable, machine-readable model enumeration
//! command. Model entry therefore remains free text behind this boundary until
//! upstream publishes one; do not couple Jefe to Code Puppy's private config.

use std::ffi::OsString;

use crate::domain::{AgentKind, LaunchSignature};

use super::RuntimeError;
use super::agent_executable::{AgentExecutableResolver, AgentExecutableTarget};
use super::agent_launcher::command_for_executable;
use super::commands::{
    remote_tmux_command, run_command_capture, run_remote_ssh, shell_escape_single,
};

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
    if signature.agent_kind != AgentKind::CodePuppy {
        return Ok(());
    }
    let pinned = crate::domain::code_puppy_requires_uvx(&signature.code_puppy_version);
    if !pinned && signature.code_puppy_yolo.is_none() {
        return Ok(());
    }

    let (target, args) = code_puppy_help_probe(signature);
    let command_label = probe_command_label(target, &args);
    let output = if signature.remote.enabled {
        let command = remote_tmux_command(&signature.remote, &command_label);
        run_remote_ssh(&signature.remote, &command)?
    } else {
        let executable = AgentExecutableResolver::current()
            .resolve_target(target)
            .map_err(RuntimeError::AgentExecutable)?;
        let arguments = args.iter().map(OsString::from).collect::<Vec<_>>();
        run_command_capture(
            command_for_executable(&executable, &arguments),
            &command_label,
        )?
    };

    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    validate_code_puppy_help(
        output.status.success(),
        &help,
        &command_label,
        signature.code_puppy_yolo.is_some(),
    )
}

fn code_puppy_help_probe(signature: &LaunchSignature) -> (AgentExecutableTarget, Vec<String>) {
    match crate::domain::code_puppy_uvx_from_spec(&signature.code_puppy_version) {
        None => (
            AgentExecutableTarget::Agent(AgentKind::CodePuppy),
            vec!["--help".to_owned()],
        ),
        Some(from_spec) => (
            AgentExecutableTarget::Uvx,
            vec![
                "--from".to_owned(),
                from_spec,
                AgentKind::CodePuppy.binary_name().to_owned(),
                "--help".to_owned(),
            ],
        ),
    }
}

fn probe_command_label(target: AgentExecutableTarget, args: &[String]) -> String {
    std::iter::once(target.binary_name().to_owned())
        .chain(args.iter().cloned())
        .map(|argument| shell_escape_single(&argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_code_puppy_help(
    success: bool,
    help: &str,
    command_label: &str,
    require_yolo: bool,
) -> Result<(), RuntimeError> {
    if !success {
        return Err(RuntimeError::CapabilityProbeFailed(format!(
            "`{command_label}` exited unsuccessfully: {}",
            bounded_probe_diagnostic(help)
        )));
    }
    if !require_yolo || code_puppy_help_supports_yolo(help) {
        return Ok(());
    }
    let selected = if command_label.contains("code-puppy==") {
        format!("Selected Code Puppy launch `{command_label}`")
    } else {
        "Code Puppy on the launch target".to_owned()
    };
    Err(RuntimeError::CapabilityCheckFailed(format!(
        "{selected} does not advertise `--yolo true|false`. Upgrade Code Puppy or edit the agent with a supported version before launching."
    )))
}

fn bounded_probe_diagnostic(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "no diagnostic was returned".to_owned()
    } else {
        value.chars().take(512).collect()
    }
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
            code_puppy_version: String::new(),
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
            llxprt_version: None,
        }
    }

    #[test]
    fn pinned_help_probe_targets_exact_uvx_package_structurally() {
        let mut pinned = signature(AgentKind::CodePuppy, Some(true));
        pinned.code_puppy_version = "  0.0.361;$(nope)  ".to_owned();
        let (target, args) = code_puppy_help_probe(&pinned);
        assert_eq!(target, super::super::AgentExecutableTarget::Uvx);
        assert_eq!(
            args,
            vec![
                "--from",
                "code-puppy==0.0.361;$(nope)",
                "code-puppy",
                "--help"
            ]
        );
    }

    #[test]
    fn blank_help_probe_remains_direct_code_puppy() {
        let direct = signature(AgentKind::CodePuppy, Some(true));
        let (target, args) = code_puppy_help_probe(&direct);
        assert_eq!(
            target,
            super::super::AgentExecutableTarget::Agent(AgentKind::CodePuppy)
        );
        assert_eq!(args, vec!["--help"]);
    }

    #[test]
    fn failed_pinned_help_diagnostic_names_exact_selection() {
        let result = validate_code_puppy_help(
            false,
            "package import failed",
            "uvx --from code-puppy==0.0.361 code-puppy --help",
            true,
        );
        let Err(error) = result else {
            panic!("failed package execution must fail capability validation");
        };
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("code-puppy==0.0.361"));
        assert!(diagnostic.contains("package import failed"));
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
        let result = validate_code_puppy_help(true, "--model MODEL", "code-puppy --help", true);
        assert!(matches!(
            result,
            Err(RuntimeError::CapabilityCheckFailed(_))
        ));
        assert!(matches!(
            validate_code_puppy_help(false, "--yolo {true,false}", "code-puppy --help", true,),
            Err(RuntimeError::CapabilityProbeFailed(_))
        ));
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

    #[test]
    fn latest_sentinel_help_probe_uses_bare_uvx_package() {
        let mut sentinel = signature(AgentKind::CodePuppy, Some(true));
        sentinel.code_puppy_version = "latest".to_owned();
        let (target, args) = code_puppy_help_probe(&sentinel);
        assert_eq!(target, super::super::AgentExecutableTarget::Uvx);
        // Bare package — no "==latest" suffix
        assert_eq!(args, vec!["--from", "code-puppy", "code-puppy", "--help"]);
    }

    #[test]
    fn latest_nightly_sentinel_help_probe_uses_bare_uvx_package() {
        let mut sentinel = signature(AgentKind::CodePuppy, Some(true));
        sentinel.code_puppy_version = "latest nightly".to_owned();
        let (target, args) = code_puppy_help_probe(&sentinel);
        assert_eq!(target, super::super::AgentExecutableTarget::Uvx);
        assert_eq!(args, vec!["--from", "code-puppy", "code-puppy", "--help"]);
    }
}
