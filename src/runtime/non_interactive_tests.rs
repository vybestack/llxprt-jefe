//! Tests for non-interactive argv construction (issue #214).

use super::non_interactive_argv;
use crate::domain::{
    AgentTypeId, AgentLaunchRequest, LlxprtNpmPackageSelector, RemoteRepositorySettings,
    SandboxEngine,
};
use crate::runtime::AgentExecutableTarget;
use std::path::PathBuf;

fn signature(kind: AgentTypeId) -> AgentLaunchRequest {
    AgentLaunchRequest {
        type_id: (kind),
        values: crate::domain::TypedMap::new(),
        work_dir: PathBuf::new(),
        remote: RemoteRepositorySettings::default(),
            operation: crate::domain::agent_definition::Operation::Normal,
        }
}

#[test]
fn llxprt_direct_uses_prompt_flag_with_instruction() {
    let (target, args) = non_interactive_argv(&signature(crate::domain::shipped_agent_type(3)), "rewrite this");
    assert_eq!(target, AgentExecutableTarget::Agent(crate::domain::shipped_agent_type(3)));
    assert_eq!(args, vec!["--prompt", "rewrite this"]);
}

#[test]
fn llxprt_includes_profile_before_prompt() {
    let mut sig = signature(crate::domain::shipped_agent_type(3));
    sig.profile = "my-profile".to_owned();
    let (target, args) = non_interactive_argv(&sig, "do it");
    assert_eq!(target, AgentExecutableTarget::Agent(crate::domain::shipped_agent_type(3)));
    assert_eq!(
        args,
        vec!["--profile-load", "my-profile", "--prompt", "do it"]
    );
}

#[test]
fn llxprt_never_passes_continue() {
    let mut sig = signature(crate::domain::shipped_agent_type(3));
    sig.pass_continue = true;
    let (_, args) = non_interactive_argv(&sig, "x");
    assert!(
        !args.iter().any(|a| a == "--continue"),
        "non-interactive run must always be fresh"
    );
}

#[test]
fn llxprt_strips_parameterized_continue_form() {
    // A mode flag like --continue=true must also be stripped so continuation
    // never leaks into a non-interactive rewrite run.
    let mut sig = signature(crate::domain::shipped_agent_type(3));
    sig.mode_flags = vec!["--continue=true".to_owned(), "--verbose".to_owned()];
    let (_, args) = non_interactive_argv(&sig, "x");
    assert!(
        !args.iter().any(|a| a.starts_with("--continue")),
        "parameterized --continue must be stripped: {args:?}"
    );
    assert!(
        args.contains(&"--verbose".to_owned()),
        "unrelated mode flags must survive the continue filter"
    );
}

#[test]
fn llxprt_includes_mode_flags_and_sandbox() {
    let mut sig = signature(crate::domain::shipped_agent_type(3));
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
    let mut sig = signature(crate::domain::shipped_agent_type(3));
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
    let (target, args) = non_interactive_argv(&signature(crate::domain::shipped_agent_type(1)), "rewrite");
    assert_eq!(target, AgentExecutableTarget::Agent(crate::domain::shipped_agent_type(1)));
    assert_eq!(args, vec!["--prompt", "rewrite"]);
}

#[test]
fn code_puppy_appends_model_and_yolo() {
    let mut sig = signature(crate::domain::shipped_agent_type(1));
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
    let mut sig = signature(crate::domain::shipped_agent_type(1));
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
    // The pure argv helper keeps the `npm exec` form so remote and legacy
    // paths stay consistent with `commands::launch_target_and_args`. The
    // local versioned path (`run_non_interactive`) builds the inner argv
    // directly (see `llxprt_npm_backed_local_run_resolves_cached_binary_argv`).
    let mut sig = signature(crate::domain::shipped_agent_type(3));
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

#[test]
fn llxprt_npm_backed_local_run_resolves_cached_binary_argv() {
    // Issue #425: the local versioned run_non_interactive path builds the
    // inner llxprt argv (no `exec --package` wrapper) and resolves the
    // cached binary from the jefe-managed install dir. This test exercises
    // the pure argv half (`non_interactive_inner_args`) to assert the local
    // form: it must contain `--prompt` and the instruction, and must NOT
    // contain the `npm exec` wrapper.
    let mut sig = signature(crate::domain::shipped_agent_type(3));
    sig.llxprt_version = LlxprtNpmPackageSelector::normalize("1.0.0");
    let inner = super::non_interactive_inner_args(&sig, "rewrite this issue");
    assert!(inner.contains(&"--prompt".to_owned()));
    assert!(inner.contains(&"rewrite this issue".to_owned()));
    assert!(
        !inner
            .iter()
            .any(|a| a == "exec" || a.starts_with("--package=")),
        "local versioned inner argv must not carry the npm exec wrapper: {inner:?}"
    );
}

#[test]
fn read_rewrite_output_file_returns_trimmed_contents() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let path = dir.path().join("out.md");
    std::fs::write(&path, "  Title\nBody  \n").unwrap_or_else(|error| panic!("write: {error}"));
    let text = super::read_rewrite_output_file(&path, None)
        .unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(text, "Title\nBody");
}

#[test]
fn read_rewrite_output_file_rejects_empty_with_stderr_hint() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let path = dir.path().join("empty.md");
    std::fs::write(&path, "   \n").unwrap_or_else(|error| panic!("write: {error}"));
    let err = match super::read_rewrite_output_file(&path, Some("model noise")) {
        Ok(text) => panic!("empty file must fail, got: {text}"),
        Err(error) => error,
    };
    let message = err.to_string();
    assert!(
        message.contains("empty rewrite output file"),
        "message={message}"
    );
    assert!(
        message.contains("model noise"),
        "stderr hint must surface: {message}"
    );
}

#[test]
fn read_rewrite_output_file_rejects_missing_path() {
    let path = std::path::Path::new("/tmp/jefe-rewrite-missing-does-not-exist.md");
    let err = match super::read_rewrite_output_file(path, None) {
        Ok(text) => panic!("missing file must fail, got: {text}"),
        Err(error) => error,
    };
    assert!(
        err.to_string().contains("did not write rewrite output"),
        "message={err}"
    );
}
