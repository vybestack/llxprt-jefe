//! Shared typed runtime launch orchestration for issue and pull-request sends.

use std::path::Path;
use std::time::Duration;

use jefe::domain::AgentId;
use jefe::runtime::launch_compose::PreparedLaunch;
use jefe::runtime::{RuntimeError, RuntimeManager};

/// Spawn a fresh session and attach it without erasing either runtime failure.
pub(super) fn spawn_and_attach_fresh<M: RuntimeManager>(
    runtime: &mut M,
    agent_id: &AgentId,
    _work_dir: &Path,
    prepared: &PreparedLaunch,
    settle_delay: Duration,
) -> Result<(), RuntimeError> {
    runtime.spawn_session_fresh(agent_id, prepared.authorized(), prepared.remote())?;
    if !settle_delay.is_zero() {
        std::thread::sleep(settle_delay);
    }
    runtime.attach(agent_id)
}
