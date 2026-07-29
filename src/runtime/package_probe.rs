//! Package availability error compatibility at the runtime boundary.

use crate::domain::{AgentLaunchRequest, LLXPRT_NPM_PACKAGE};

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

/// Validate package/candidate availability after support and generation checks.
pub fn require_launch_package_available(
    request: &AgentLaunchRequest,
) -> Result<(), super::RuntimeError> {
    super::launch_compose::observe_launch_state(request).map(|_| ())
}
