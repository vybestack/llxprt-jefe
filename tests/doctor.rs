//! Integration tests for the `jefe doctor` command (issue #264, PR 1 / S1).
//!
//! This is a dedicated integration-test target (rather than lib-registered
//! tests) because the lib test registration budget is already over the soft
//! threshold — see `tests/git_info.rs` for the same organizational pattern.
//!
//! The submodules are pulled in via `#[path = ...]` so each contract area stays
//! in its own focused file under 750 lines while compiling as a single binary.
//!
//! Scope of this RED slice (PR 1 / S1):
//! - typed CLI parse of `doctor` with optional `--config <dir>` and rejection
//!   of extra operands/options (no `--json` / `--copy`);
//! - typed diagnostic statuses / exit classification (required startup blockers
//!   fail, optional Git/gh/agent runtime findings warn, internal probe failure
//!   maps to diagnostic command error);
//! - redaction of usernames, raw SIDs, home paths, URL userinfo/passwords,
//!   token/credential-shaped values, and prompt/credential evidence while
//!   retaining structural path/actionable labels;
//! - human report rendering includes Jefe version/commit, platform/architecture,
//!   multiplexer, namespace, ConPTY, Git, gh/auth, LLxprt Code, Code Puppy,
//!   config/state, and long-path sections and applies redaction before
//!   rendering;
//! - a read-only persistence probe contract that leaves a missing config
//!   directory absent and cleans transient probes in existing writable dirs.
//!
//! These tests reference APIs under `jefe::doctor` and `jefe::cli` that
//! production has not exposed yet. The target is expected NOT to compile until
//! those APIs land; that failure is the RED proof for this slice.

#[path = "doctor/support.rs"]
mod support;

#[path = "doctor/cli.rs"]
mod cli;
#[path = "doctor/diagnostics.rs"]
mod diagnostics;
#[path = "doctor/persistence_probe.rs"]
mod persistence_probe;
#[path = "doctor/redaction.rs"]
mod redaction;
#[path = "doctor/report.rs"]
mod report;
#[path = "doctor/windows_probe.rs"]
mod windows_probe;
