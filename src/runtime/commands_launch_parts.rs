//! Issue #425 local-launch argv selection, split out of `commands.rs` so the
//! parent file stays under the source-file-size hard limit.
//!
//! Pure functions: no I/O, no environment reads. The plan builder in
//! `commands.rs` calls `local_launch_parts` to decide the executable target,
//! pane args, and optional managed bin dir for a signature.

use std::path::PathBuf;

use crate::domain::AgentKind;
use crate::domain::LaunchSignature;
use crate::runtime::agent_executable::AgentExecutableTarget;

/// Select the local launch form for a signature.
///
/// Returns `(executable, args, managed_bin_dir)`:
/// - Code Puppy uvx launch: `(Uvx, --from/binary/inner_args, None)`.
/// - Direct (unversioned LLxprt or code-puppy on PATH):
///   `(Agent(kind), inner_args, None)`.
/// - Local versioned LLxprt launch (issue #425): `(Agent(Llxprt), inner_args,
///   Some(managed_bin_dir))`. The cached `llxprt` binary runs directly from
///   jefe's managed install dir instead of `npm exec`, so the work dir's
///   `node_modules` cannot shadow the pinned version and concurrent launches
///   cannot contend on the `_npx` cache lock.
pub(super) fn local_launch_parts(
    signature: &LaunchSignature,
    inner_args: Vec<String>,
) -> (AgentExecutableTarget, Vec<String>, Option<PathBuf>) {
    if let Some(from_spec) = crate::domain::code_puppy_uvx_from_spec(&signature.code_puppy_version)
    {
        let mut args = vec![
            "--from".to_owned(),
            from_spec,
            AgentKind::CodePuppy.binary_name().to_owned(),
        ];
        args.extend(inner_args);
        return (AgentExecutableTarget::Uvx, args, None);
    }
    match versioned_local_selector(signature) {
        None => (
            AgentExecutableTarget::Agent(signature.agent_kind),
            inner_args,
            None,
        ),
        Some(selector) => (
            AgentExecutableTarget::Agent(AgentKind::Llxprt),
            inner_args,
            Some(crate::runtime::llxprt_install::bin_dir_for(selector)),
        ),
    }
}

/// Whether a remote session is enabled. Mirrors `commands::remote_is_enabled`
/// so this pure module does not depend on the commands module.
fn remote_is_enabled(remote: &crate::domain::RemoteRepositorySettings) -> bool {
    crate::domain::target::is_valid_remote(remote)
}

/// The selector for a local versioned LLxprt launch, when the launch path
/// should run the jefe-managed cached binary instead of resolving on PATH or
/// using `npm exec`. Returns `None` for direct launches, code-puppy launches,
/// and remote launches (which keep the `npm exec` form).
pub(super) fn versioned_local_selector(
    signature: &LaunchSignature,
) -> Option<&crate::domain::LlxprtNpmPackageSelector> {
    if remote_is_enabled(&signature.remote) {
        return None;
    }
    if !matches!(signature.agent_kind, AgentKind::Llxprt) {
        return None;
    }
    signature.llxprt_version.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LlxprtNpmPackageSelector;

    fn llxprt_signature(version: Option<&str>) -> LaunchSignature {
        let mut sig = test_signature_base();
        sig.agent_kind = AgentKind::Llxprt;
        sig.llxprt_version = version.map(|v| {
            LlxprtNpmPackageSelector::normalize(v)
                .unwrap_or_else(|| panic!("selector fixture must normalize: {v}"))
        });
        sig
    }

    fn test_signature_base() -> LaunchSignature {
        LaunchSignature {
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: "default".to_owned(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: crate::domain::RemoteRepositorySettings::default(),
            agent_kind: AgentKind::Llxprt,
            llxprt_version: None,
        }
    }

    #[test]
    fn direct_launch_has_no_managed_bin_dir() {
        let sig = llxprt_signature(None);
        let (target, _args, managed) = local_launch_parts(&sig, vec!["--continue".to_owned()]);
        assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt));
        assert!(managed.is_none());
    }

    #[test]
    fn versioned_local_launch_sets_managed_bin_dir() {
        let sig = llxprt_signature(Some("0.9.0"));
        let (target, args, managed) = local_launch_parts(&sig, vec!["--continue".to_owned()]);
        assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt));
        assert_eq!(args, vec!["--continue".to_owned()]);
        let bin_dir = managed.unwrap_or_else(|| panic!("managed bin dir set"));
        assert!(bin_dir.components().any(|c| c.as_os_str() == ".bin"));
        assert!(bin_dir.components().any(|c| c.as_os_str() == "0.9.0"));
    }

    #[test]
    fn versioned_remote_launch_has_no_managed_bin_dir() {
        let mut sig = llxprt_signature(Some("0.9.0"));
        sig.remote.enabled = true;
        sig.remote.login_user = "ubuntu".to_owned();
        sig.remote.host = "linux.example".to_owned();
        let (target, _args, managed) = local_launch_parts(&sig, vec!["--continue".to_owned()]);
        assert_eq!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt));
        assert!(
            managed.is_none(),
            "remote launches keep npm exec, no managed dir"
        );
    }
}
