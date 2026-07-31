//! JSP launch instrumentation for the runtime manager.
//!
//! Owns the decision of whether a spawn is instrumented, the preparation of
//! owner-only bootstrap material, and lifecycle revocation. Keeping this here
//! leaves `manager` responsible for session bookkeeping only.

use tracing::debug;

use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::jsp_host::{JspLaunchCoordinator, PreparedJspLaunch};
use crate::runtime::agent_preflight::{AuthorizedLaunchPlan, ProcessSandboxInspector};
use crate::runtime::errors::RuntimeError;

/// Prepare an instrumented launch plan.
///
/// Returns the plan unchanged when instrumentation does not apply: no
/// coordinator is installed, the session is being reattached rather than
/// spawned, or the launch does not support JSP.
pub(super) fn prepare(
    coordinator: Option<&JspLaunchCoordinator>,
    agent_id: &AgentId,
    launch: &AuthorizedLaunchPlan,
    remote: Option<&RemoteRepositorySettings>,
    reattaching: bool,
    generation: u64,
) -> Result<(AuthorizedLaunchPlan, Option<PreparedJspLaunch>), RuntimeError> {
    debug!(
        agent_id = %agent_id.0,
        generation,
        type_id = %launch.plan().type_id,
        remote = remote.is_some(),
        reattaching,
        jsp_installed = coordinator.is_some(),
        "preparing JSP launch instrumentation"
    );
    if reattaching {
        return Ok((launch.clone(), None));
    }
    let Some(coordinator) = coordinator else {
        return Ok((launch.clone(), None));
    };
    let prepared = coordinator
        .prepare_launch(agent_id, generation, launch.plan())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let Some(prepared) = prepared else {
        debug!(agent_id = %agent_id.0, "launch does not support JSP instrumentation");
        return Ok((launch.clone(), None));
    };
    debug!(agent_id = %agent_id.0, generation, "prepared JSP bootstrap");
    let instrumented = launch
        .with_jsp_bootstrap(prepared.bootstrap_path(), &ProcessSandboxInspector::new())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    Ok((instrumented, Some(prepared)))
}

/// Revoke any credentials bound to `agent_id`.
///
/// Cleanup failure is reported and otherwise ignored: the caller is tearing a
/// session down and must continue regardless.
pub(super) fn revoke(coordinator: Option<&JspLaunchCoordinator>, agent_id: &AgentId) {
    if let Some(coordinator) = coordinator
        && let Err(error) = coordinator.revoke(agent_id)
    {
        debug!(agent_id = %agent_id.0, error = %error, "JSP lifecycle cleanup failed");
    }
}
