//! Shared typed runtime launch orchestration for issue and pull-request sends.

use std::path::Path;
use std::time::Duration;

use jefe::domain::{AgentId, AgentLaunchRequest};
use jefe::runtime::{RuntimeError, RuntimeManager};

/// Spawn a fresh session and attach it without erasing either runtime failure.
pub(super) fn spawn_and_attach_fresh<M: RuntimeManager>(
    runtime: &mut M,
    agent_id: &AgentId,
    _work_dir: &Path,
    signature: &AgentLaunchRequest,
    settle_delay: Duration,
) -> Result<(), RuntimeError> {
    let (plan, remote) = jefe::runtime::launch_compose::plan_from_request(signature)?;
    runtime.spawn_session_fresh(agent_id, &plan, remote.as_ref())?;
    if !settle_delay.is_zero() {
        std::thread::sleep(settle_delay);
    }
    runtime.attach(agent_id)
}
