//! Focused integration tests for the persistent provider candidate lifecycle
//! (issue #390 CW-10, Slice C2: CW10-03 ordered startup, CW10-04 atomic
//! publication with rollback reap, no auto-restart, explicit shutdown, and the
//! CW10-11/14 cleanup-evidence / redaction / strict-ack remediation).
//!
//! This is a dedicated integration-test target (rather than lib-registered
//! tests) because the lib test registration budget is already over the soft
//! threshold — see `tests/doctor.rs` and `tests/git_info.rs` for the same
//! `#[path = ...]` split organization.
//!
//! The submodules are pulled in via `#[path = ...]` so each contract area stays
//! in its own focused file under the 750-line source-size gate while compiling
//! as a single binary:
//! - [`support`]: shared fixture `Scene`, deterministic host environments, fast
//!   bounds, and bounded poll/reap helpers.
//! - [`lifecycle`]: ordered startup, atomic publication, rollback reap at every
//!   handshake phase, no auto-restart, explicit shutdown reap, duplicate-id.
//! - [`remediation`]: secret redaction, drain timeout, strict shutdown-ack,
//!   fail-fast health, post-exit pipe closure, and the shutdown-frame write
//!   failure.

#[path = "issue390_persistent_providers/lifecycle.rs"]
mod lifecycle;
#[path = "issue390_persistent_providers/remediation.rs"]
mod remediation;
#[path = "issue390_persistent_providers/support.rs"]
mod support;
