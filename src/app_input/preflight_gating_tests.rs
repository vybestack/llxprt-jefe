//! Tests for sandbox preflight gating by agent kind (finding 3).
//!
//! CodePuppy does not use the LLxprt sandbox subsystem. A CodePuppy agent
//! may carry stale `sandbox_enabled = true` / `sandbox_engine` from
//! persisted edit data; preflight must NOT fire for it. LLxprt with
//! sandbox_enabled must still fire preflight.

use super::preflight::should_run_sandbox_preflight;
use std::path::PathBuf;

use jefe::domain::{
    AgentKind, DEFAULT_SANDBOX_FLAGS, LaunchSignature, RemoteRepositorySettings, SandboxEngine,
};

fn sample_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: PathBuf::from("/tmp/agent"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: Some(false),
        code_puppy_quick_resume: false,
        mode_flags: vec![],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
        llxprt_version: None,
    }
}

#[test]
fn code_puppy_with_sandbox_enabled_skips_preflight() {
    let mut sig = sample_signature();
    sig.agent_kind = AgentKind::CodePuppy;
    sig.sandbox_enabled = true;
    sig.sandbox_engine = SandboxEngine::Podman;
    assert!(
        !should_run_sandbox_preflight(&sig),
        "CodePuppy must skip sandbox preflight even with sandbox_enabled"
    );
}

#[test]
fn llxprt_with_sandbox_enabled_runs_preflight() {
    let mut sig = sample_signature();
    sig.agent_kind = AgentKind::Llxprt;
    sig.sandbox_enabled = true;
    sig.sandbox_engine = SandboxEngine::Podman;
    assert!(
        should_run_sandbox_preflight(&sig),
        "LLxprt with sandbox_enabled must run preflight"
    );
}

#[test]
fn llxprt_with_sandbox_disabled_skips_preflight() {
    let mut sig = sample_signature();
    sig.agent_kind = AgentKind::Llxprt;
    sig.sandbox_enabled = false;
    assert!(
        !should_run_sandbox_preflight(&sig),
        "LLxprt with sandbox disabled must skip preflight"
    );
}

#[test]
fn code_puppy_with_sandbox_disabled_skips_preflight() {
    let mut sig = sample_signature();
    sig.agent_kind = AgentKind::CodePuppy;
    sig.sandbox_enabled = false;
    assert!(
        !should_run_sandbox_preflight(&sig),
        "CodePuppy with sandbox disabled must skip preflight"
    );
}
