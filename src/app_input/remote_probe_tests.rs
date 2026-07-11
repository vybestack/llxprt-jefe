//! Tests for the remote agent-runtime availability probe and classifier
//! (defects 2, 3, 4).
//!
//! These tests exercise:
//! - The pure [`classify_probe_output`] classifier: true/false/transport/auth/
//!   effective-user/malformed-output scenarios.
//! - The pure [`plan_remote_probe`] planner: ssh -T, exact binary, effective
//!   user, no install/setup, sentinel protocol.
//! - The centralized [`require_runtime_available`] validation: unavailable
//!   remote means no prep/prompt operation.
//! - The production PR prompt planning seam [`plan_remote_prompt_write`]:
//!   `.jefe/pr-prompt.md` targeted, prompt bytes in stdin, adversarial
//!   content absent from argv.

use super::*;
use std::path::Path;

use jefe::domain::{AgentKind, RemoteRepositorySettings};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
    fn error_or_panic(self, context: &str) -> String;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }

    fn error_or_panic(self, context: &str) -> String {
        match self {
            Ok(_) => panic!("{context}: expected error"),
            Err(error) => format!("{error:?}"),
        }
    }
}

fn valid_remote() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "build.example.com".to_owned(),
        run_as_user: String::new(),
        setup_env_default: false,
    }
}

fn remote_with_run_as() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "build.example.com".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: false,
    }
}

// ── classify_probe_output: pure classifier (defect 3) ────────────────

#[test]
fn classify_ok_sentinel_is_available() {
    let result = classify_probe_output(Some(0), "JEFE_PROBE_OK", "");
    assert_eq!(result, RemoteProbeResult::Available);
    assert!(matches!(result, RemoteProbeResult::Available));
}

#[test]
fn classify_no_sentinel_is_not_available() {
    let result = classify_probe_output(Some(0), "JEFE_PROBE_NO", "");
    assert_eq!(result, RemoteProbeResult::NotAvailable);
    assert!(!matches!(result, RemoteProbeResult::Available));
}

#[test]
fn classify_ok_sentinel_with_trailing_newline_is_available() {
    // ssh may add a trailing newline.
    let result = classify_probe_output(Some(0), "JEFE_PROBE_OK\n", "");
    assert_eq!(result, RemoteProbeResult::Available);
}

// ── Exact sentinel matching (defect 2: reject prefix/suffix/banner/both) ─

#[test]
fn classify_prefix_before_sentinel_is_error() {
    // A banner/prefix before the sentinel must NOT be accepted.
    let result = classify_probe_output(Some(0), "Welcome to Ubuntu\nJEFE_PROBE_OK", "");
    assert!(
        matches!(result, RemoteProbeResult::Error(_)),
        "prefix before sentinel must be rejected: {result:?}"
    );
}

#[test]
fn classify_suffix_after_sentinel_is_error() {
    // Trailing content after the sentinel (beyond whitespace) must be rejected.
    let result = classify_probe_output(Some(0), "JEFE_PROBE_OK\nextra line", "");
    assert!(
        matches!(result, RemoteProbeResult::Error(_)),
        "suffix after sentinel must be rejected: {result:?}"
    );
}

#[test]
fn classify_both_sentinels_is_error() {
    // Both OK and NO must never appear; exact match prevents this.
    let result = classify_probe_output(Some(0), "JEFE_PROBE_OK\nJEFE_PROBE_NO", "");
    assert!(
        matches!(result, RemoteProbeResult::Error(_)),
        "both sentinels must be rejected: {result:?}"
    );
}

#[test]
fn classify_partial_sentinel_is_error() {
    // A substring of a sentinel is NOT the exact sentinel.
    let result = classify_probe_output(Some(0), "JEFE_PROBE_O", "");
    assert!(
        matches!(result, RemoteProbeResult::Error(_)),
        "partial sentinel must be rejected: {result:?}"
    );
}

#[test]
fn classify_no_sentinel_with_leading_whitespace_is_not_available() {
    // Whitespace is trimmed, so leading spaces around a bare NO are fine.
    let result = classify_probe_output(Some(0), "  JEFE_PROBE_NO  ", "");
    assert_eq!(result, RemoteProbeResult::NotAvailable);
}

#[test]
fn classify_ok_sentinel_embedded_in_sentence_is_error() {
    // The sentinel must be the ENTIRE trimmed output, not a substring.
    let result = classify_probe_output(Some(0), "The result is JEFE_PROBE_OK for sure", "");
    assert!(
        matches!(result, RemoteProbeResult::Error(_)),
        "embedded sentinel must be rejected: {result:?}"
    );
}

#[test]
fn classify_ssh_exit_255_is_transport_error() {
    let result = classify_probe_output(Some(255), "", "Permission denied (publickey).");
    assert!(matches!(result, RemoteProbeResult::Error(ref msg)
        if msg.contains("transport") || msg.contains("auth") || msg.contains("255")));
}

#[test]
fn classify_exit_0_no_sentinel_is_malformed_error() {
    // Exit 0 but no sentinel — protocol mismatch or truncated output.
    // Must be an error, NOT NotAvailable (never trigger a clone).
    let result = classify_probe_output(Some(0), "garbage output", "");
    assert!(matches!(result, RemoteProbeResult::Error(_)));
}

#[test]
fn classify_exit_0_empty_output_is_malformed_error() {
    let result = classify_probe_output(Some(0), "", "");
    assert!(matches!(result, RemoteProbeResult::Error(_)));
}

#[test]
fn classify_nonzero_exit_is_error() {
    let result = classify_probe_output(Some(1), "", "some error");
    assert!(matches!(result, RemoteProbeResult::Error(_)));
}

#[test]
fn classify_signal_terminated_is_error() {
    let result = classify_probe_output(None, "", "");
    assert!(matches!(result, RemoteProbeResult::Error(_)));
}

#[test]
fn classify_not_available_never_triggers_clone() {
    // NotAvailable is a clean false predicate — it must NOT be confused
    // with Available or Error.
    let result = classify_probe_output(Some(0), "JEFE_PROBE_NO", "");
    assert_ne!(result, RemoteProbeResult::Available);
    assert!(!matches!(result, RemoteProbeResult::Error(_)));
}

#[test]
fn classify_transport_error_never_triggers_clone() {
    // Transport failure must be Error, NOT Available or NotAvailable.
    let result = classify_probe_output(Some(255), "", "ssh: connect to host: Connection refused");
    assert!(!matches!(result, RemoteProbeResult::Available));
    assert!(!matches!(result, RemoteProbeResult::NotAvailable));
}

// ── effective_user (defect 2: probe as effective user) ──────────────

#[test]
fn effective_user_defaults_to_login_user() {
    let remote = valid_remote();
    assert_eq!(effective_user(&remote), "ubuntu");
}

#[test]
fn effective_user_uses_run_as_user_when_set() {
    let remote = remote_with_run_as();
    assert_eq!(effective_user(&remote), "acoliver");
}

#[test]
fn effective_user_trims_whitespace() {
    let remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "  ubuntu  ".to_owned(),
        host: "host".to_owned(),
        run_as_user: "  acoliver  ".to_owned(),
        setup_env_default: false,
    };
    assert_eq!(effective_user(&remote), "acoliver");
}

// ── plan_remote_probe: pure planning (defect 2) ─────────────────────

#[test]
fn probe_plan_uses_ssh_t_not_tt() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    assert!(argv.iter().any(|a| a == "-T"), "must use -T: {argv:?}");
    assert!(
        !argv.iter().any(|a| a == "-tt"),
        "must not use -tt: {argv:?}"
    );
}

#[test]
fn probe_plan_uses_batch_mode_and_connect_timeout() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    assert!(argv.iter().any(|a| a == "BatchMode=yes"));
    assert!(argv.iter().any(|a| a == "ConnectTimeout=10"));
    // Non-interactive host-key policy: auto-accept on first connect so the
    // probe never hangs waiting for user input.
    assert!(argv.iter().any(|a| a == "StrictHostKeyChecking=accept-new"));
    // Post-connect keepalive so a hung remote session is detected within
    // ~15s instead of blocking indefinitely.
    assert!(argv.iter().any(|a| a == "ServerAliveInterval=5"));
    assert!(argv.iter().any(|a| a == "ServerAliveCountMax=3"));
}

#[test]
fn probe_plan_targets_login_user_at_host() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    assert!(argv.iter().any(|a| a == "ubuntu@build.example.com"));
}

#[test]
fn probe_plan_probes_exact_code_puppy_binary() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    let Some(command) = argv.iter().find(|a| a.contains("command -v")) else {
        panic!("must have command -v: {argv:?}");
    };
    assert!(
        command.contains("code-puppy"),
        "must probe exact binary code-puppy: {argv:?}"
    );
    assert!(
        !command.contains("code_puppy"),
        "must not use underscore form: {argv:?}"
    );
}

#[test]
fn probe_plan_probes_exact_llxprt_binary() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    let Some(command) = argv.iter().find(|a| a.contains("command -v")) else {
        panic!("must have command -v: {argv:?}");
    };
    assert!(
        command.contains("llxprt"),
        "must probe exact binary llxprt: {argv:?}"
    );
}

// ── LLxprt path-local probe (defect 3: mirror launch resolver) ───────

#[test]
fn probe_plan_llxprt_includes_path_local_node_modules_bin() {
    // LLxprt probe must mirror the launch resolver: global command -v OR
    // executable <work_dir>/node_modules/.bin/llxprt.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    assert!(
        command.contains("/home/ubuntu/work/node_modules/.bin/llxprt"),
        "LLxprt probe must include path-local node_modules/.bin/llxprt: {argv:?}"
    );
}

#[test]
fn probe_plan_llxprt_uses_or_between_global_and_path_local() {
    // The probe must accept EITHER global command OR path-local executable,
    // mirroring the non-mutating launch resolver checks.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    // Both the global command -v and the path-local [ -x ... ] must appear.
    assert!(
        command.contains("command -v llxprt"),
        "must probe global llxprt: {argv:?}"
    );
    assert!(
        command.contains("[ -x ") && command.contains("node_modules/.bin/llxprt"),
        "must probe path-local executable: {argv:?}"
    );
}

#[test]
fn probe_plan_code_puppy_does_not_include_path_local() {
    // CodePuppy probe must remain global-only (launch resolver has no
    // path-local fallback for code-puppy).
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    assert!(
        !command.contains("node_modules/.bin"),
        "CodePuppy probe must NOT include path-local: {argv:?}"
    );
    assert!(
        command.contains("command -v code-puppy"),
        "CodePuppy probe must use global command -v: {argv:?}"
    );
}

#[test]
fn probe_plan_llxprt_escapes_work_dir_in_path_local() {
    // The work_dir must be safely shell-escaped in the path-local path so
    // adversarial paths cannot inject shell commands. The shell_escape
    // function wraps the entire path in single quotes and escapes internal
    // single quotes, so adversarial metacharacters are inert.
    let adversarial_work = "/home/ubuntu/wo'rk; rm -rf /";
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new(adversarial_work),
        AgentKind::Llxprt,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    // The path-local must be present and properly single-quoted. The key
    // safety property: the `;` and `rm -rf` must be inside single quotes
    // (shell-inert), not unescaped shell separators. We verify by checking
    // the escaped form appears (the `'` before `;` proves it's quoted).
    assert!(
        command.contains("node_modules/.bin/llxprt"),
        "must still reference node_modules/.bin/llxprt: {argv:?}"
    );
    // The raw unescaped `; rm -rf /` (with a space before rm and a real
    // command separator) must not appear as an unquoted shell command. The
    // escaped form wraps it in single quotes, so there should be no
    // bare `; rm -rf` outside quotes.
    // The shell_escape output for the path would be like:
    //   '/home/ubuntu/wo'\''rk; rm -rf //node_modules/.bin/llxprt'
    // The `'\''` is the escaped single-quote — the content after it is
    // still within the re-opened single quote, so it's inert.
    assert!(
        command.contains("'\\''"),
        "adversarial work_dir must be single-quote escaped: {argv:?}"
    );
}

#[test]
fn probe_plan_llxprt_no_install_or_setup() {
    // The path-local probe must remain side-effect-free — no npm install,
    // no setup-env, no package managers.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    for arg in &argv {
        assert!(
            !arg.contains("npm install"),
            "probe must not install: {arg}"
        );
        assert!(
            !arg.contains("setup-env"),
            "probe must not setup-env: {arg}"
        );
        assert!(
            !arg.contains("npm install @vybestack"),
            "probe must not install llxprt-code: {arg}"
        );
    }
}

#[test]
fn probe_plan_llxprt_path_local_uses_executable_check_not_existence() {
    // Must use [ -x ... ] (executable), not just existence — a non-executable
    // file is not a usable binary.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    assert!(
        command.contains("[ -x "),
        "path-local check must use [ -x ] (executable): {argv:?}"
    );
}

#[test]
fn probe_plan_does_not_run_install_or_setup() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    for arg in &argv {
        assert!(
            !arg.contains("npm install"),
            "probe must not install: {arg}"
        );
        assert!(
            !arg.contains("setup-env"),
            "probe must not setup-env: {arg}"
        );
        assert!(
            !arg.contains("apt-get") && !arg.contains("brew install"),
            "probe must not use package managers: {arg}"
        );
    }
}

#[test]
fn probe_plan_uses_sentinel_protocol() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    let Some(command) = argv.iter().find(|a| a.contains("JEFE_PROBE")) else {
        panic!("must use sentinel: {argv:?}");
    };
    assert!(command.contains("JEFE_PROBE_OK"));
    assert!(command.contains("JEFE_PROBE_NO"));
}

#[test]
fn probe_plan_wraps_effective_user_when_run_as_differs() {
    let argv = plan_remote_probe(
        &remote_with_run_as(),
        Path::new("/home/acoliver/work"),
        AgentKind::CodePuppy,
    );
    let Some(command) = argv.iter().find(|a| a.contains("sudo")) else {
        panic!("must wrap in sudo: {argv:?}");
    };
    assert!(
        command.contains("acoliver"),
        "must run as effective user acoliver: {argv:?}"
    );
}

#[test]
fn probe_plan_no_sudo_when_effective_equals_login() {
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    assert!(
        !argv.iter().any(|a| a.contains("sudo")),
        "must not use sudo when effective==login: {argv:?}"
    );
}

#[test]
fn probe_plan_does_not_cd_to_work_dir_for_global_check() {
    // The global binary check must NOT cd to work_dir — the work directory
    // may not exist yet (clone-if-missing flow), and a globally-installed
    // runtime must be detected regardless. The path-local `[ -x ... ]` check
    // for LLxprt uses an absolute path and is safe without cd.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::CodePuppy,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("command -v"))
        .unwrap_or_else(|| panic!("must have command -v: {argv:?}"));
    assert!(
        !command.contains("cd "),
        "CodePuppy probe must NOT cd to work dir (global check only): {argv:?}"
    );
    assert!(
        command.starts_with("command -v code-puppy"),
        "global check must run first without cd: {argv:?}"
    );
}

#[test]
fn probe_plan_llxprt_global_check_has_no_cd() {
    // LLxprt global check must also run without cd — a missing workdir must
    // not mask a global install.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        AgentKind::Llxprt,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    assert!(
        !command.contains("cd "),
        "LLxprt probe must NOT cd to work dir: {argv:?}"
    );
    assert!(
        command.starts_with("{ command -v llxprt"),
        "global check must run first without cd: {argv:?}"
    );
}

#[test]
fn probe_plan_code_puppy_works_without_existing_workdir() {
    // Even with a non-existent work dir, the global probe must be a valid
    // command (no cd that would fail). The exact work_dir value is
    // irrelevant for CodePuppy.
    let argv = plan_remote_probe(
        &valid_remote(),
        Path::new("/nonexistent/path"),
        AgentKind::CodePuppy,
    );
    let command = argv
        .iter()
        .find(|a| a.contains("JEFE_PROBE"))
        .unwrap_or_else(|| panic!("must have sentinel command: {argv:?}"));
    assert!(
        !command.contains("/nonexistent/path"),
        "CodePuppy probe must not reference work_dir at all: {argv:?}"
    );
}

// ── require_runtime_available: centralized pre-side-effect validation ─

#[test]
fn require_local_available_passes_when_installed() {
    let target = WorkTarget::Local;
    let result = require_runtime_available(
        &target,
        Path::new("/tmp/work"),
        AgentKind::CodePuppy,
        &[AgentKind::CodePuppy],
    );
    assert!(result.is_ok());
}

#[test]
fn require_local_fails_when_not_installed() {
    let target = WorkTarget::Local;
    let result = require_runtime_available(
        &target,
        Path::new("/tmp/work"),
        AgentKind::CodePuppy,
        &[AgentKind::Llxprt],
    );
    let err = result.error_or_panic("local availability check should fail");
    assert!(err.contains("code_puppy"));
    assert!(err.contains("PATH"));
}

/// Unavailable remote means no prep/prompt operation — but we cannot execute
/// a real SSH probe in unit tests. Instead, we verify that the centralized
/// validator delegates to the remote probe path and would return an error
/// for a non-connectable host. This proves the seam exists and that a
/// non-available remote blocks.
///
/// Since `execute_remote_probe` does a real ssh, we test the classification
/// path via the pure classifier instead (see `classify_*` tests above). Here
/// we verify the local path is wired correctly.
#[test]
fn require_runtime_local_wires_to_local_check() {
    let target = WorkTarget::Local;
    let available = &[AgentKind::Llxprt];
    // CodePuppy not in local snapshot → error.
    assert!(
        require_runtime_available(
            &target,
            Path::new("/tmp/work"),
            AgentKind::CodePuppy,
            available
        )
        .is_err()
    );
    // Llxprt in snapshot → ok.
    assert!(
        require_runtime_available(
            &target,
            Path::new("/tmp/work"),
            AgentKind::Llxprt,
            available
        )
        .is_ok()
    );
}

// ── plan_remote_prompt_write: PR prompt planning seam (defect 4) ─────

#[test]
fn pr_prompt_plan_targets_jefe_pr_prompt_path() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "PR prompt content",
    )
    .value_or_panic("valid plan");
    assert!(
        plan.ssh_argv
            .iter()
            .any(|a| a.contains(".jefe/pr-prompt.md")),
        "must target .jefe/pr-prompt.md: {:?}",
        plan.ssh_argv
    );
    // Must NOT target the issue path.
    assert!(
        plan.ssh_argv
            .iter()
            .all(|a| !a.contains(".jefe/issue-prompt.md")),
        "must NOT target issue path: {:?}",
        plan.ssh_argv
    );
}

#[test]
fn pr_prompt_plan_not_targets_issue_path() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    for arg in &plan.ssh_argv {
        assert!(
            !arg.contains("issue-prompt"),
            "PR plan must not reference issue prompt: {arg}"
        );
    }
}

#[test]
fn pr_prompt_plan_prompt_bytes_in_stdin_not_argv() {
    let adversarial = "'; rm -rf /; echo '`\n$(whoami)";
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        adversarial,
    )
    .value_or_panic("valid plan");
    // Prompt bytes are in stdin_prompt.
    assert_eq!(plan.stdin_prompt, adversarial);
    // No adversarial content in argv.
    for arg in &plan.ssh_argv {
        assert!(
            !arg.contains("rm -rf"),
            "adversarial content must not appear in argv: {arg}"
        );
        assert!(
            !arg.contains("whoami"),
            "adversarial content must not appear in argv: {arg}"
        );
    }
}

#[test]
fn pr_prompt_plan_uses_cat_redirect_for_stdin_transfer() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert!(
        plan.ssh_argv.iter().any(|a| a.contains("cat >")),
        "must use cat > for stdin transfer: {:?}",
        plan.ssh_argv
    );
}

#[test]
fn pr_prompt_plan_creates_jefe_dir() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert!(
        plan.ssh_argv.iter().any(|a| a.contains("mkdir -p")),
        "must create .jefe dir: {:?}",
        plan.ssh_argv
    );
}

#[test]
fn pr_prompt_plan_uses_ssh_t() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert!(plan.ssh_argv.iter().any(|a| a == "-T"));
    assert!(!plan.ssh_argv.iter().any(|a| a == "-tt"));
}

#[test]
fn pr_prompt_plan_targets_login_user_at_host() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert!(
        plan.ssh_argv
            .iter()
            .any(|a| a == "ubuntu@build.example.com"),
        "must target ubuntu@build.example.com: {:?}",
        plan.ssh_argv
    );
}

#[test]
fn pr_prompt_plan_wraps_effective_user() {
    let plan = plan_remote_prompt_write(
        &remote_with_run_as(),
        Path::new("/home/acoliver/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert!(
        plan.ssh_argv.iter().any(|a| a.contains("sudo")),
        "must wrap effective user: {:?}",
        plan.ssh_argv
    );
    assert!(
        plan.ssh_argv.iter().any(|a| a.contains("acoliver")),
        "must run as acoliver: {:?}",
        plan.ssh_argv
    );
}

#[test]
fn pr_prompt_plan_rejects_absolute_path() {
    let result = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        "/etc/passwd",
        "content",
    );
    assert!(result.is_err());
}

#[test]
fn pr_prompt_plan_rejects_traversal_path() {
    let result = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        ".jefe/../../../etc/passwd",
        "content",
    );
    assert!(result.is_err());
}

#[test]
fn pr_prompt_plan_rejects_non_jefe_path() {
    let result = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        "etc/evil.md",
        "content",
    );
    assert!(result.is_err());
}

#[test]
fn pr_prompt_relative_path_constant_has_correct_value() {
    // The PR seam must target .jefe/pr-prompt.md, NOT the issue path.
    // Verify the constant value that all PR prompt writes use.
    assert_eq!(PR_PROMPT_RELATIVE_PATH, ".jefe/pr-prompt.md");
}

#[test]
fn pr_prompt_plan_relative_path_recorded() {
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        "content",
    )
    .value_or_panic("valid plan");
    assert_eq!(plan.relative_path, ".jefe/pr-prompt.md");
}

#[test]
fn pr_prompt_plan_adversarial_newlines_in_stdin_only() {
    let adversarial = "line1\nline2\n'; injected '; `backtick`";
    let plan = plan_remote_prompt_write(
        &valid_remote(),
        Path::new("/home/ubuntu/work"),
        PR_PROMPT_RELATIVE_PATH,
        adversarial,
    )
    .value_or_panic("valid plan");
    assert_eq!(plan.stdin_prompt, adversarial);
    for arg in &plan.ssh_argv {
        assert!(
            !arg.contains("injected"),
            "adversarial must not leak into argv: {arg}"
        );
    }
}
