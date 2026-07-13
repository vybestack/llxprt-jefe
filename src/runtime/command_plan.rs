//! Target-neutral executable-selection command plan.
//!
//! This module owns the single abstraction that decides how an agent session
//! is launched: whether the agent binary runs directly or is routed through
//! `npm exec --yes --package=@vybestack/llxprt-code@SELECTOR -- llxprt`.
//!
//! The plan is a pure data structure derived from a [`LaunchSignature`]. It
//! never performs I/O and never spawns processes — it produces the exact argv
//! tokens for local launches and the shell-escaped command string for remote
//! launches.
//!
//! ## Branch on AgentKind before selector
//!
//! Code Puppy is always a direct plan regardless of any dormant LLxprt
//! selector value. Only LLxprt agents consult the version selector. This is
//! the structural guard that prevents Code Puppy from ever invoking npm
//! because of a dormant LLxprt selector.
//!
//! ## Blank means direct
//!
//! A blank (after trim) LLxprt version preserves the existing direct/resolved
//! `llxprt` launch behavior. A nonblank version routes through npm exec.

use crate::domain::{AgentKind, LaunchSignature};

/// The npm package name prefix for versioned LLxprt launches.
const LLXPRT_PACKAGE_PREFIX: &str = "@vybestack/llxprt-code@";

/// The fixed argv prefix for a versioned LLxprt launch:
/// `npm exec --yes --package=@vybestack/llxprt-code@VERSION -- llxprt`.
///
/// The `--package=@vybestack/llxprt-code@VERSION` is a single argv token
/// (verified npm exec syntax). The package_token field carries that whole
/// token; the fixed field carries everything before it.
struct NpmExecPrefix {
    fixed: Vec<&'static str>,
    /// The full `--package=@vybestack/llxprt-code@VERSION` token.
    package_token: String,
}

/// How the agent executable is selected for a launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutablePlan {
    /// Launch the agent binary directly (e.g. `llxprt` or `code-puppy`).
    Direct,
    /// Launch LLxprt through `npm exec --yes --package=... -- llxprt`.
    NpmExec {
        /// The trimmed npm package selector (e.g. `0.9.0`,
        /// `0.10.0-nightly.260712.21cb698b6`).
        version: String,
    },
}

impl ExecutablePlan {
    /// Derive the executable plan from a launch signature.
    ///
    /// Code Puppy is always `Direct`. LLxprt is `NpmExec` when the version
    /// selector is nonblank after trimming, and `Direct` otherwise.
    #[must_use]
    pub fn from_signature(signature: &LaunchSignature) -> Self {
        // Branch on AgentKind BEFORE consulting the selector so Code Puppy
        // never invokes npm from dormant LLxprt data.
        match signature.agent_kind {
            AgentKind::CodePuppy => Self::Direct,
            AgentKind::Llxprt => {
                let version = signature.llxprt_version.trim();
                if version.is_empty() {
                    Self::Direct
                } else {
                    Self::NpmExec {
                        version: version.to_owned(),
                    }
                }
            }
        }
    }

    /// Whether this plan routes the launch through `npm exec`.
    #[must_use]
    pub const fn requires_npm(&self) -> bool {
        matches!(self, Self::NpmExec { .. })
    }

    /// Build the argv prefix tokens for a local launch.
    ///
    /// For [`ExecutablePlan::Direct`], this is empty — the caller appends the
    /// binary name and its arguments.
    ///
    /// For [`ExecutablePlan::NpmExec`], this returns the full
    /// `npm exec --yes --package=@vybestack/llxprt-code@VERSION -- llxprt`
    /// sequence as distinct tokens. The `--package=...` value is a single
    /// token (no spaces) so it survives as one argv element.
    #[must_use]
    pub fn local_argv_prefix(
        &self,
        npm_executable: Option<&std::path::Path>,
    ) -> Vec<std::ffi::OsString> {
        match self {
            Self::Direct => Vec::new(),
            Self::NpmExec { version } => {
                let prefix = Self::npm_exec_prefix(version);
                let npm = npm_executable.map_or_else(
                    || std::ffi::OsString::from("npm"),
                    |path| path.as_os_str().to_owned(),
                );
                let mut args = vec![npm];
                args.extend(prefix.fixed.iter().skip(1).map(std::ffi::OsString::from));
                args.push(prefix.package_token.into());
                args.push("--".into());
                args.push("llxprt".into());
                args
            }
        }
    }

    /// Build the shell-escaped command prefix for a remote launch.
    ///
    /// For [`ExecutablePlan::Direct`], this is empty — the caller uses the
    /// resolved remote binary path.
    ///
    /// For [`ExecutablePlan::NpmExec`], this returns the full npm exec
    /// invocation with every token shell-escaped via single-quote escaping.
    #[must_use]
    pub fn remote_command_prefix(&self, npm_executable: &str) -> String {
        match self {
            Self::Direct => String::new(),
            Self::NpmExec { version } => {
                let prefix = Self::npm_exec_prefix(version);
                let mut tokens = vec![npm_executable.to_owned()];
                tokens.extend(prefix.fixed.iter().skip(1).map(|s| (*s).to_owned()));
                tokens.push(prefix.package_token);
                tokens.push("--".to_owned());
                tokens.push("llxprt".to_owned());
                tokens
                    .iter()
                    .map(|t| shell_escape_single(t))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }

    /// Build the fixed npm exec prefix parts and the package token.
    ///
    /// The `--package=@vybestack/llxprt-code@VERSION` is a single token per
    /// verified npm exec syntax (not separate `--package` and value tokens).
    fn npm_exec_prefix(version: &str) -> NpmExecPrefix {
        NpmExecPrefix {
            fixed: vec!["npm", "exec", "--yes"],
            package_token: format!("--package={LLXPRT_PACKAGE_PREFIX}{version}"),
        }
    }
}

/// Shell-escape a single token using single-quote wrapping.
///
/// Delegates to the runtime's shared shell-escaping implementation so npm
/// prefix tokens and trailing launch arguments cannot drift.
fn shell_escape_single(value: &str) -> String {
    super::commands::shell_escape_single(value)
}

// ── Launch argument planning ───────────────────────────────────────────────
//
// The launch-args family is the pure, kind-specific translation from a
// [`LaunchSignature`] into the trailing argv tokens that follow the
// executable (or the npm exec prefix). These functions never spawn a process
// or touch the filesystem — they are unit-testable without a tmux runtime.

/// Derive the kind-specific launch argument tokens from a signature.
pub(super) fn launch_args(signature: &LaunchSignature) -> Vec<String> {
    match signature.agent_kind {
        AgentKind::CodePuppy => code_puppy_launch_args(signature),
        AgentKind::Llxprt => llxprt_launch_args(signature),
    }
}

/// Code Puppy launch args: `-i`, optional `--quick-resume`, optional explicit
/// model, optional YOLO, and for fresh sends one positional instruction.
pub(super) fn code_puppy_launch_args(signature: &LaunchSignature) -> Vec<String> {
    let mut args = vec!["-i".to_owned()];
    if signature.code_puppy_quick_resume {
        args.push("--quick-resume".to_owned());
        args.push(signature.work_dir.to_string_lossy().into_owned());
    }
    if !signature.code_puppy_model.trim().is_empty() {
        args.push("--model".to_owned());
        args.push(signature.code_puppy_model.trim().to_owned());
    }
    if let Some(yolo) = signature.code_puppy_yolo {
        args.push("--yolo".to_owned());
        args.push(yolo.to_string());
    }
    // Fresh issue/PR preparation replaces mode_flags with exactly one
    // positional instruction and disables pass_continue. Require that exact
    // shape here so arbitrary persisted LLxprt flags, multiple arguments, and
    // option-looking values never leak into Code Puppy.
    if !signature.pass_continue
        && let [instruction] = signature.mode_flags.as_slice()
        && !instruction.starts_with('-')
    {
        args.push(instruction.clone());
    }
    args
}

/// LLxprt launch args: optional profile load, mode flags, continue, sandbox.
pub(super) fn llxprt_launch_args(signature: &LaunchSignature) -> Vec<String> {
    let mut args = Vec::new();
    if !signature.profile.is_empty() {
        args.push("--profile-load".to_owned());
        args.push(signature.profile.clone());
    }
    args.extend(
        signature
            .mode_flags
            .iter()
            .filter(|flag| !flag.is_empty())
            .cloned(),
    );
    if signature.pass_continue {
        args.push("--continue".to_owned());
    }
    if signature.sandbox_enabled {
        args.push("--sandbox".to_owned());
        args.push("--sandbox-engine".to_owned());
        args.push(signature.sandbox_engine.as_llxprt_arg().to_owned());
    }
    args
}

/// Resolved local launch plan: the executable selection plan, kind-specific
/// args, environment assignments, and any preflight warning.
///
/// Moved here from `commands.rs` to keep that file under the source-size hard
/// limit. The plan is pure data derived from a [`LaunchSignature`]; the actual
/// tmux session creation stays in `commands.rs`.
pub(super) struct LocalLaunchPlan {
    pub(super) agent_kind: AgentKind,
    pub(super) plan: ExecutablePlan,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) warning: Option<String>,
}

/// Build a [`LocalLaunchPlan`] from a [`LaunchSignature`].
pub(super) fn local_launch_plan(signature: &LaunchSignature) -> LocalLaunchPlan {
    let mut env = Vec::new();
    let warning = match signature.agent_kind {
        AgentKind::Llxprt => {
            if signature.sandbox_enabled {
                env.push(("SANDBOX_FLAGS".to_owned(), signature.sandbox_flags.clone()));
                if let Some(image_ref) = std::env::var_os("LLXPRT_SANDBOX_IMAGE") {
                    env.push((
                        "LLXPRT_SANDBOX_IMAGE".to_owned(),
                        image_ref.to_string_lossy().into_owned(),
                    ));
                }
                super::preflight::sandbox_ssh_agent_warning()
            } else {
                None
            }
        }
        AgentKind::CodePuppy => None,
    };
    if matches!(signature.agent_kind, AgentKind::Llxprt) && !signature.llxprt_debug.is_empty() {
        env.push(("LLXPRT_DEBUG".to_owned(), signature.llxprt_debug.clone()));
    }
    LocalLaunchPlan {
        agent_kind: signature.agent_kind,
        plan: ExecutablePlan::from_signature(signature),
        args: launch_args(signature),
        env,
        warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RemoteRepositorySettings, SandboxEngine};
    use std::path::PathBuf;

    use ExecutablePlan::NpmExec;

    fn llxprt_sig(version: &str) -> LaunchSignature {
        LaunchSignature {
            work_dir: PathBuf::from("/tmp/work"),
            profile: String::new(),
            code_puppy_model: String::new(),
            llxprt_version: version.to_owned(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: AgentKind::Llxprt,
        }
    }

    fn puppy_sig(version: &str) -> LaunchSignature {
        let mut sig = llxprt_sig(version);
        sig.agent_kind = AgentKind::CodePuppy;
        sig
    }

    #[test]
    fn blank_llxprt_version_is_direct() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig(""));
        assert_eq!(plan, ExecutablePlan::Direct);
        assert!(!plan.requires_npm());
        assert!(plan.local_argv_prefix(None).is_empty());
        assert!(plan.remote_command_prefix("npm").is_empty());
    }

    #[test]
    fn whitespace_only_llxprt_version_is_direct() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig("   "));
        assert_eq!(plan, ExecutablePlan::Direct);
    }

    #[test]
    fn stable_version_produces_npm_exec_argv() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig("0.9.0"));
        assert_eq!(
            plan,
            ExecutablePlan::NpmExec {
                version: "0.9.0".to_owned()
            }
        );
        assert!(plan.requires_npm());
        assert_eq!(
            plan.local_argv_prefix(None),
            vec![
                "npm",
                "exec",
                "--yes",
                "--package=@vybestack/llxprt-code@0.9.0",
                "--",
                "llxprt",
            ]
        );
    }

    #[test]
    fn versioned_launch_uses_resolved_npm_executable() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig("0.9.0"));
        let argv = plan.local_argv_prefix(Some(std::path::Path::new("/opt/node/bin/npm")));
        assert_eq!(
            argv.first().map(std::ffi::OsString::as_os_str),
            Some(std::ffi::OsStr::new("/opt/node/bin/npm"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn versioned_launch_preserves_non_utf8_npm_executable_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let plan = ExecutablePlan::from_signature(&llxprt_sig("0.9.0"));
        let npm = std::ffi::OsString::from_vec(b"/tmp/npm-\xff".to_vec());
        let argv = plan.local_argv_prefix(Some(std::path::Path::new(&npm)));
        let Some(executable) = argv.first() else {
            panic!("npm executable must be the first argv token");
        };
        assert_eq!(executable.as_os_str().as_bytes(), b"/tmp/npm-\xff");
    }
    #[test]
    fn nightly_selector_preserved_exactly() {
        let nightly = "0.10.0-nightly.260712.21cb698b6";
        let plan = ExecutablePlan::from_signature(&llxprt_sig(nightly));
        let NpmExec { version } = &plan else {
            panic!("expected NpmExec for nightly selector");
        };
        assert_eq!(version, nightly);
        let expected = std::ffi::OsString::from(format!(
            "--package={}{}",
            super::LLXPRT_PACKAGE_PREFIX,
            nightly
        ));
        assert!(plan.local_argv_prefix(None).contains(&expected));
    }

    #[test]
    fn surrounding_whitespace_trimmed() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig("  0.9.0  "));
        let NpmExec { version } = &plan else {
            panic!("expected NpmExec");
        };
        assert_eq!(version, "0.9.0");
    }

    #[test]
    fn code_puppy_always_direct_regardless_of_dormant_version() {
        let plan = ExecutablePlan::from_signature(&puppy_sig("0.9.0"));
        assert_eq!(plan, ExecutablePlan::Direct);
        assert!(!plan.requires_npm());
    }

    #[test]
    fn remote_command_prefix_shell_escapes_every_token() {
        let plan = ExecutablePlan::from_signature(&llxprt_sig("0.9.0"));
        let cmd = plan.remote_command_prefix("npm");
        // Every token must be single-quoted.
        for token in cmd.split(' ') {
            assert!(
                token.starts_with('\'') && token.ends_with('\''),
                "token {token:?} must be single-quoted in remote prefix"
            );
        }
        assert!(cmd.contains("'--package=@vybestack/llxprt-code@0.9.0'"));
        assert!(cmd.contains("'npm'"));
        assert!(cmd.contains("'exec'"));
        assert!(cmd.contains("'--yes'"));
        assert!(cmd.contains("'--'"));
        assert!(cmd.contains("'llxprt'"));
    }

    #[test]
    fn adversarial_selector_kept_as_one_local_token() {
        // An adversarial selector must be one argv token, not split by spaces.
        // The `--package=` form ensures the selector is a single argv element
        // that the shell/process never interprets as separate commands.
        let adversarial = "0.9.0; rm -rf /";
        let plan = ExecutablePlan::from_signature(&llxprt_sig(adversarial));
        let argv = plan.local_argv_prefix(None);
        let expected = std::ffi::OsString::from(format!(
            "--package={}{}",
            super::LLXPRT_PACKAGE_PREFIX,
            adversarial
        ));
        let Some(package_token) = argv.iter().find(|token| *token == &expected) else {
            panic!("single --package= token must be present, got {argv:?}");
        };
        assert_eq!(package_token, &expected);
        // The adversarial command must never appear as a standalone binary
        // token (e.g. `rm` must not be its own argv element).
        assert!(!argv.iter().any(|t| t == "rm" || t == "-rf"));
        // The package token must be exactly one element.
        assert_eq!(argv.iter().filter(|token| **token == expected).count(), 1);
    }

    #[test]
    fn direct_plan_produces_empty_prefixes() {
        let plan = ExecutablePlan::from_signature(&puppy_sig(""));
        assert!(plan.local_argv_prefix(None).is_empty());
        assert!(plan.remote_command_prefix("npm").is_empty());
    }
}
