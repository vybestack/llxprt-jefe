//! Session-cached detection of installed agent runtimes.

use std::sync::OnceLock;

use crate::domain::AgentKind;
use crate::runtime::{AgentExecutablePlatform, AgentExecutableResolver};

static INSTALLED_AGENT_KINDS: OnceLock<Vec<AgentKind>> = OnceLock::new();

/// Agent kinds whose executable is present on PATH, detected once per session.
#[must_use]
pub fn installed_agent_kinds() -> &'static [AgentKind] {
    INSTALLED_AGENT_KINDS.get_or_init(detect_installed_agent_kinds)
}

fn detect_installed_agent_kinds() -> Vec<AgentKind> {
    detect_with_resolver(&AgentExecutableResolver::current())
}

/// Pure detection of which agent runtimes are installed, given an explicit
/// slice of PATH directories.
///
/// Returns the kinds whose executable is present and executable (on Unix) or
/// present as a file (on non-Unix) in any of the supplied directories. The
/// detection order follows the canonical kind order in the candidate list.
///
/// Extracted as a pure function so the detection logic is deterministically
/// testable without touching the real filesystem or `PATH` environment
/// variable.
#[must_use]
pub fn detect_agent_kinds(dirs: &[std::path::PathBuf]) -> Vec<AgentKind> {
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        dirs.to_vec(),
        std::env::var_os("PATHEXT"),
    );
    detect_with_resolver(&resolver)
}

fn detect_with_resolver(resolver: &AgentExecutableResolver) -> Vec<AgentKind> {
    [AgentKind::Llxprt, AgentKind::CodePuppy]
        .into_iter()
        .filter(|kind| resolver.resolve(*kind).is_ok())
        .collect()
}
