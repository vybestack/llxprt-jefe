//! RED-first remediation tests for the handle-free provider request state
//! (issue #390 CW-10, Slice B).
//!
//! The submodules are pulled in via `#[path = ...]` so each contract area
//! stays in its own focused file under the 750-line source-size gate while
//! compiling as one module:
//! - [`support`]: shared test helpers (ids, policies, invoke/outcome builders).
//! - [`confirmation`]: policy/outcome validation and the confirmation
//!   round-trip (invocation B carries exact original data/continuation).
//! - [`protocol`]: post-terminal bytes, progress fault, generation exhaustion,
//!   and the no-duplicate-queue invariant.
//! - [`lifecycle`]: confirm atomicity (generation counter), retry of an
//!   unknown old key, and cancel-after-terminal no-effect semantics.

#[path = "provider_request_red_tests/confirmation.rs"]
mod confirmation;
#[path = "provider_request_red_tests/lifecycle.rs"]
mod lifecycle;
#[path = "provider_request_red_tests/protocol.rs"]
mod protocol;
#[path = "provider_request_red_tests/support.rs"]
mod support;
