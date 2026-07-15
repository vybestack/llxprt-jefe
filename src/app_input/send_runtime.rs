//! Shared typed runtime launch orchestration for issue and pull-request sends.

use std::path::Path;
use std::time::Duration;

use jefe::domain::{AgentId, LaunchSignature};
use jefe::runtime::{RuntimeError, RuntimeManager};

/// Spawn a fresh session and attach it without erasing either runtime failure.
pub(super) fn spawn_and_attach_fresh<M: RuntimeManager>(
    runtime: &mut M,
    agent_id: &AgentId,
    work_dir: &Path,
    signature: &LaunchSignature,
    settle_delay: Duration,
) -> Result<(), RuntimeError> {
    runtime.spawn_session_fresh(agent_id, work_dir, signature)?;
    if !settle_delay.is_zero() {
        std::thread::sleep(settle_delay);
    }
    runtime.attach(agent_id)
}

#[cfg(test)]
mod tests {
    use super::spawn_and_attach_fresh;
    use jefe::domain::{
        AgentId, AgentKind, LaunchSignature, LlxprtNpmPackageSelector, RemoteRepositorySettings,
        SandboxEngine,
    };
    use jefe::runtime::{NpmPackageAvailabilityError, RuntimeError, StubRuntimeManager};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn signature() -> LaunchSignature {
        LaunchSignature {
            work_dir: PathBuf::from("/tmp/send-runtime"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: AgentKind::Llxprt,
            llxprt_version: LlxprtNpmPackageSelector::normalize("nightly"),
        }
    }

    #[test]
    fn package_race_error_survives_send_runtime_boundary() {
        let availability = NpmPackageAvailabilityError::PackageUnresolved {
            target: "local machine".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "version disappeared".to_owned(),
        };
        let expected = RuntimeError::NpmPackageAvailability(availability.clone()).to_string();
        let mut runtime = StubRuntimeManager::with_spawn_failure(
            RuntimeError::NpmPackageAvailability(availability),
        );

        let error = spawn_and_attach_fresh(
            &mut runtime,
            &AgentId("send-agent".to_owned()),
            Path::new("/tmp/send-runtime"),
            &signature(),
            Duration::ZERO,
        )
        .err()
        .unwrap_or_else(|| panic!("package race must fail the send launch"));

        assert_eq!(error.to_string(), expected);
        assert!(error.to_string().contains("version disappeared"));
    }
}
