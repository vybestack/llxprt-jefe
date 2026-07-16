//! Non-installing availability probe for selector-backed LLxprt packages.

use std::ffi::OsString;
use std::process::Output;
use std::time::Duration;

use crate::domain::{
    LLXPRT_NPM_PACKAGE, LaunchSignature, LaunchSource, LlxprtNpmPackageSelector,
    llxprt_launch_source,
};

use super::agent_executable::{
    AgentExecutableError, AgentExecutableResolver, AgentExecutableTarget,
};
use super::agent_launcher::command_for_executable;
use super::commands::{remote_tmux_command, run_command_capture_with_timeout, shell_escape_single};

const LOCAL_PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DIAGNOSTIC_BYTES: usize = 512;
const NPM_MISSING_SENTINEL: &str = "JEFE_NPM_MISSING";

/// Failure to probe npm or resolve the requested LLxprt package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpmPackageAvailabilityError {
    /// npm is absent from the effective target's PATH.
    NpmMissing {
        /// Effective local or remote target.
        target: String,
        /// Requested npm selector.
        selector: String,
    },
    /// The probe could not be planned or started locally.
    ProbeFailure {
        /// Effective target.
        target: String,
        /// Requested npm selector.
        selector: String,
        /// Bounded failure detail.
        diagnostic: String,
    },
    /// SSH planning, authentication, timeout, or transport failed.
    TransportFailure {
        /// Effective target.
        target: String,
        /// Requested npm selector.
        selector: String,
        /// Bounded failure detail.
        diagnostic: String,
    },
    /// The probe process ended without an actionable exit code.
    ExecutionFailure {
        /// Effective target.
        target: String,
        /// Requested npm selector.
        selector: String,
        /// Bounded failure detail.
        diagnostic: String,
    },
    /// npm ran but could not resolve the package selector.
    PackageUnresolved {
        /// Effective local or remote target.
        target: String,
        /// Requested npm selector.
        selector: String,
        /// Bounded npm diagnostic.
        diagnostic: String,
    },
}

impl std::fmt::Display for NpmPackageAvailabilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NpmMissing { target, selector } => write!(
                formatter,
                "npm is not available on {target} for LLxprt selector '{selector}'; install Node.js with npm on that target or clear the LLxprt version selector"
            ),
            Self::ProbeFailure {
                target,
                selector,
                diagnostic,
            } => write!(
                formatter,
                "could not start the npm availability probe for LLxprt selector '{selector}' on {target}; verify the local npm installation and retry. diagnostic: {diagnostic}"
            ),
            Self::TransportFailure {
                target,
                selector,
                diagnostic,
            } => write!(
                formatter,
                "could not reach {target} to check LLxprt selector '{selector}'; verify SSH settings, authentication, and connectivity, then retry. diagnostic: {diagnostic}"
            ),
            Self::ExecutionFailure {
                target,
                selector,
                diagnostic,
            } => write!(
                formatter,
                "the npm availability probe for LLxprt selector '{selector}' on {target} did not complete normally; retry after checking the target process environment. diagnostic: {diagnostic}"
            ),
            Self::PackageUnresolved {
                target,
                selector,
                diagnostic,
            } => write!(
                formatter,
                "npm could not resolve {LLXPRT_NPM_PACKAGE}@{selector} on {target}; verify the selector and registry access or clear the LLxprt version selector. npm diagnostic: {diagnostic}"
            ),
        }
    }
}

impl std::error::Error for NpmPackageAvailabilityError {}

fn local_probe_arguments(selector: &LlxprtNpmPackageSelector) -> Vec<String> {
    vec![
        "view".to_owned(),
        "--json".to_owned(),
        selector.package_spec(),
        "version".to_owned(),
    ]
}

fn remote_probe_script(selector: &LlxprtNpmPackageSelector) -> String {
    let arguments = local_probe_arguments(selector)
        .iter()
        .map(|argument| shell_escape_single(argument))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "if ! command -v npm >/dev/null 2>&1; then printf '%s' {NPM_MISSING_SENTINEL}; exit 42; fi; npm {arguments}"
    )
}

fn effective_remote_target(signature: &LaunchSignature) -> String {
    let remote = &signature.remote;
    let user = if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim()
    } else {
        remote.run_as_user.trim()
    };
    format!("{user}@{}", remote.host.trim())
}

fn bounded_diagnostic(value: &str) -> String {
    let trimmed = value.trim();
    let end = trimmed
        .char_indices()
        .take_while(|(index, character)| index + character.len_utf8() <= MAX_DIAGNOSTIC_BYTES)
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    match &trimmed[..end] {
        "" => "no diagnostic was returned".to_owned(),
        bounded => bounded.to_owned(),
    }
}

fn failure_fields(
    target: &str,
    selector: &LlxprtNpmPackageSelector,
    diagnostic: &str,
) -> (String, String, String) {
    (
        target.to_owned(),
        selector.as_str().to_owned(),
        bounded_diagnostic(diagnostic),
    )
}

fn unresolved_error(
    target: &str,
    selector: &LlxprtNpmPackageSelector,
    diagnostic: &str,
) -> NpmPackageAvailabilityError {
    let (target, selector, diagnostic) = failure_fields(target, selector, diagnostic);
    NpmPackageAvailabilityError::PackageUnresolved {
        target,
        selector,
        diagnostic,
    }
}

fn classify_probe(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    target: &str,
    selector: &LlxprtNpmPackageSelector,
    remote: bool,
) -> Result<(), NpmPackageAvailabilityError> {
    if exit_code == Some(0) {
        return Ok(());
    }
    if exit_code == Some(42) && stdout.trim() == NPM_MISSING_SENTINEL {
        return Err(NpmPackageAvailabilityError::NpmMissing {
            target: target.to_owned(),
            selector: selector.as_str().to_owned(),
        });
    }
    if exit_code.is_none() {
        let (target, selector, diagnostic) =
            failure_fields(target, selector, "probe terminated without an exit code");
        return Err(NpmPackageAvailabilityError::ExecutionFailure {
            target,
            selector,
            diagnostic,
        });
    }
    if remote && exit_code == Some(255) {
        let (target, selector, diagnostic) = failure_fields(target, selector, stderr);
        return Err(NpmPackageAvailabilityError::TransportFailure {
            target,
            selector,
            diagnostic,
        });
    }
    Err(unresolved_error(target, selector, stderr))
}

fn classify_local_probe(
    output: &Output,
    selector: &LlxprtNpmPackageSelector,
) -> Result<(), NpmPackageAvailabilityError> {
    classify_probe(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        "local machine",
        selector,
        false,
    )
}

fn classify_remote_probe(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    target: &str,
    selector: &LlxprtNpmPackageSelector,
) -> Result<(), NpmPackageAvailabilityError> {
    classify_probe(exit_code, stdout, stderr, target, selector, true)
}

fn local_resolution_error(
    selector: &LlxprtNpmPackageSelector,
    error: AgentExecutableError,
) -> NpmPackageAvailabilityError {
    match error {
        AgentExecutableError::NotFound { .. } => NpmPackageAvailabilityError::NpmMissing {
            target: "local machine".to_owned(),
            selector: selector.as_str().to_owned(),
        },
        error @ AgentExecutableError::NonCanonicalNpmWrapper { .. } => {
            let (target, selector, diagnostic) =
                failure_fields("local machine", selector, &error.to_string());
            NpmPackageAvailabilityError::ProbeFailure {
                target,
                selector,
                diagnostic,
            }
        }
    }
}

fn require_local_with_resolver(
    selector: &LlxprtNpmPackageSelector,
    resolver: &AgentExecutableResolver,
    timeout: Duration,
) -> Result<(), NpmPackageAvailabilityError> {
    let executable = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .map_err(|error| local_resolution_error(selector, error))?;
    let arguments = local_probe_arguments(selector)
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let output = run_command_capture_with_timeout(
        command_for_executable(&executable, &arguments),
        timeout,
        "npm package availability probe",
    )
    .map_err(|error| {
        let (target, selector, diagnostic) =
            failure_fields("local machine", selector, &error.to_string());
        NpmPackageAvailabilityError::ProbeFailure {
            target,
            selector,
            diagnostic,
        }
    })?;
    classify_local_probe(&output, selector)
}

fn require_local(selector: &LlxprtNpmPackageSelector) -> Result<(), NpmPackageAvailabilityError> {
    require_local_with_resolver(
        selector,
        &AgentExecutableResolver::current(),
        LOCAL_PROBE_TIMEOUT,
    )
}

fn require_remote(
    signature: &LaunchSignature,
    selector: &LlxprtNpmPackageSelector,
) -> Result<(), NpmPackageAvailabilityError> {
    let target = effective_remote_target(signature);
    let command = remote_tmux_command(&signature.remote, &remote_probe_script(selector));
    let plan = crate::ssh::SshPlan::new(
        &signature.remote,
        &command,
        crate::ssh::SshMode::NonInteractive,
    )
    .map_err(|error| transport_error(&target, selector, &error.to_string()))?;
    let output = plan
        .execute(None, crate::ssh::SSH_OPERATION_TIMEOUT, None)
        .map_err(|error| transport_error(&target, selector, &error.to_string()))?;
    classify_remote_probe(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        &target,
        selector,
    )
}

fn transport_error(
    target: &str,
    selector: &LlxprtNpmPackageSelector,
    diagnostic: &str,
) -> NpmPackageAvailabilityError {
    let (target, selector, diagnostic) = failure_fields(target, selector, diagnostic);
    NpmPackageAvailabilityError::TransportFailure {
        target,
        selector,
        diagnostic,
    }
}

/// Probe npm package availability for a selector-backed launch.
///
/// Direct LLxprt and Code Puppy launches perform no package probe.
pub fn require_npm_package_available(
    signature: &LaunchSignature,
) -> Result<(), NpmPackageAvailabilityError> {
    let LaunchSource::NpmBacked(selector) =
        llxprt_launch_source(signature.agent_kind, signature.llxprt_version.as_ref())
    else {
        return Ok(());
    };
    if crate::domain::target::is_valid_remote(&signature.remote) {
        require_remote(signature, &selector)
    } else {
        require_local(&selector)
    }
}

/// Probe the package boundary required by the exact launch signature.
///
/// Direct launches remain probe-free here; direct Code Puppy capability checks
/// continue through `validate_code_puppy_launch` during normal preflight.
pub fn require_launch_package_available(
    signature: &LaunchSignature,
) -> Result<(), super::RuntimeError> {
    if signature.agent_kind == crate::domain::AgentKind::CodePuppy
        && crate::domain::code_puppy_requires_uvx(&signature.code_puppy_version)
    {
        return super::capabilities::validate_code_puppy_launch(signature);
    }
    require_npm_package_available(signature).map_err(super::RuntimeError::NpmPackageAvailability)
}

#[cfg(test)]
#[path = "package_probe_tests.rs"]
mod tests;
