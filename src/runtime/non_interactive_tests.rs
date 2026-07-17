//! Tests for non-interactive argv construction (issue #214).

use super::non_interactive_argv;
use crate::domain::{
    AgentKind, LaunchSignature, LlxprtNpmPackageSelector, RemoteRepositorySettings, SandboxEngine,
};
use crate::runtime::AgentExecutableTarget;
use std::path::PathBuf;

fn signature(kind: AgentKind) -> LaunchSignature {
    LaunchSignature {
        work_dir: PathBuf::new(),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: None,
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
fn llxprt_direct_uses_prompt_flag_with_instruction() {
    let (target, args) = non_interactive_argv(&signature(AgentKind::Llxprt), "rewrite this");
    assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt));
    assert_eq!(args, vec!["--prompt", "rewrite this"]);
}

#[test]
fn llxprt_includes_profile_before_prompt() {
    let mut sig = signature(AgentKind::Llxprt);
    sig.profile = "my-profile".to_owned();
    let (target, args) = non_interactive_argv(&sig, "do it");
    assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt));
    assert_eq!(
        args,
        vec!["--profile-load", "my-profile", "--prompt", "do it"]
    );
}

#[test]
fn llxprt_never_passes_continue() {
    let mut sig = signature(AgentKind::Llxprt);
    sig.pass_continue = true;
    let (_, args) = non_interactive_argv(&sig, "x");
    assert!(
        !args.iter().any(|a| a == "--continue"),
        "non-interactive run must always be fresh"
    );
}

#[test]
fn llxprt_includes_mode_flags_and_sandbox() {
    let mut sig = signature(AgentKind::Llxprt);
    sig.mode_flags = vec!["--dangerously-skip-permissions".to_owned()];
    sig.sandbox_enabled = true;
    let (_, args) = non_interactive_argv(&sig, "x");
    assert!(args.contains(&"--dangerously-skip-permissions".to_owned()));
    assert!(args.contains(&"--sandbox".to_owned()));
    // Assert the actual engine value, not just the flag presence, so a broken
    // serialization of SandboxEngine is caught.
    let engine_idx = args
        .iter()
        .position(|a| a == "--sandbox-engine")
        .unwrap_or_else(|| panic!("--sandbox-engine flag missing"));
    assert_eq!(
        args.get(engine_idx + 1),
        Some(&sig.sandbox_engine.as_llxprt_arg().to_owned())
    );
}

#[test]
fn llxprt_strips_continue_from_mode_flags() {
    let mut sig = signature(AgentKind::Llxprt);
    sig.mode_flags = vec![
        "--continue".to_owned(),
        "--dangerously-skip-permissions".to_owned(),
    ];
    let (_, args) = non_interactive_argv(&sig, "x");
    assert!(
        !args.contains(&"--continue".to_owned()),
        "non-interactive run must always be fresh"
    );
    assert!(args.contains(&"--dangerously-skip-permissions".to_owned()));
}

#[test]
fn code_puppy_direct_uses_prompt_flag() {
    let (target, args) = non_interactive_argv(&signature(AgentKind::CodePuppy), "rewrite");
    assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::CodePuppy));
    assert_eq!(args, vec!["--prompt", "rewrite"]);
}

#[test]
fn code_puppy_appends_model_and_yolo() {
    let mut sig = signature(AgentKind::CodePuppy);
    sig.code_puppy_model = "gpt-4o".to_owned();
    sig.code_puppy_yolo = Some(true);
    let (_, args) = non_interactive_argv(&sig, "rewrite");
    assert_eq!(
        args,
        vec!["--prompt", "rewrite", "--model", "gpt-4o", "--yolo", "true"]
    );
}

#[test]
fn code_puppy_uvx_wraps_with_from_and_binary() {
    let mut sig = signature(AgentKind::CodePuppy);
    sig.code_puppy_version = "1.2.3".to_owned();
    let (target, args) = non_interactive_argv(&sig, "rewrite");
    assert_eq!(target, AgentExecutableTarget::Uvx);
    let expected_spec = format!("{}==1.2.3", crate::domain::CODE_PUPPY_PACKAGE);
    // uvx wrapper: --from <spec> code-puppy <inner args...>
    assert_eq!(
        &args[0..3],
        &["--from", expected_spec.as_str(), "code-puppy"]
    );
    assert!(args.contains(&"--prompt".to_owned()));
}

#[test]
fn llxprt_npm_backed_wraps_with_exec_package() {
    let mut sig = signature(AgentKind::Llxprt);
    sig.llxprt_version = LlxprtNpmPackageSelector::normalize("1.0.0");
    let (target, args) = non_interactive_argv(&sig, "rewrite");
    assert_eq!(target, AgentExecutableTarget::Npm);
    // Build the expected package spec from the domain constant so the test
    // tracks the production package name rather than a brittle string.
    let expected_package_spec = format!("{}@1.0.0", crate::domain::LLXPRT_NPM_PACKAGE);
    assert!(args.contains(&format!("--package={expected_package_spec}")));
    assert!(args.contains(&"llxprt".to_owned()));
    assert!(args.contains(&"--prompt".to_owned()));
}
