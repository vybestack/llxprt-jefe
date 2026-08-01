//! Issue #556 two-process installer fixture.
//!
//! Drives the production managed-install boundary
//! (`jefe::runtime::package_runtime::finalize_local_invocation`) once against a
//! caller-supplied cache root, npm fixture directory, and version selector, then
//! prints the resolved managed executable path to stdout. The cross-process
//! serialization test spawns two copies of this fixture against the same shared
//! cache to prove concurrent installers serialize (exactly one install) rather
//! than race on the cache directory.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution, VersionSelector};
use jefe::agent_candidate_path::{AgentExecutablePlatform, PathSnapshot};
use jefe::domain::agent_definition::AgentDefinition;
use jefe::runtime::package_runtime::finalize_local_invocation;

fn main() -> ExitCode {
    match run() {
        Ok(executable) => {
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{}", executable.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "jefe-issue556-installer: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf, String> {
    let cache = std::env::var_os("JEFE_ISSUE556_CACHE")
        .map(PathBuf::from)
        .ok_or_else(|| "JEFE_ISSUE556_CACHE is required".to_string())?;
    let bin_dir = std::env::var_os("JEFE_ISSUE556_BIN")
        .map(PathBuf::from)
        .ok_or_else(|| "JEFE_ISSUE556_BIN is required".to_string())?;
    let selector_str = std::env::var("JEFE_ISSUE556_SELECTOR")
        .map_err(|_| "JEFE_ISSUE556_SELECTOR is required".to_string())?;

    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.display_name == "LLxprt")
        .ok_or_else(|| "shipped LLxprt definition is missing".to_string())?;
    let selector = VersionSelector::normalize(&selector_str)
        .map_err(|error| format!("normalize selector `{selector_str}`: {error}"))?;
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir],
        std::env::var_os("PATHEXT"),
    );
    let resolution = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector)
        .resolve(&definition);
    let CandidateResolution::Resolved(candidate) = resolution else {
        return Err("package candidate did not resolve".to_string());
    };

    let invocation =
        finalize_local_invocation(&candidate, &cache).map_err(|error| error.to_string())?;
    Ok(invocation.executable().to_path_buf())
}
