//! Integration tests for the action-provider lifecycle (issue #390 CW-10).
//!
//! Every contract area for this issue compiles into **one** test binary rather
//! than one per area. Each integration-test target links the whole library, and
//! under `cargo llvm-cov` that link is instrumented and very large: five
//! separate targets were enough to exhaust a CI runner, whose OOM killer took
//! the job down mid-run (the linker died with signal 9 locally at the same
//! memory ceiling). Consolidating keeps the same tests and the same isolation
//! between areas while paying the expensive link once.
//!
//! Areas stay in their own files under the 750-line source-size gate:
//! - [`persistent_support`]: shared fixture `Scene`, deterministic host
//!   environments, fast bounds, and the bounded poll/reap helpers.
//! - [`persistent_lifecycle`]: CW10-03 ordered startup, CW10-04 atomic
//!   publication with rollback reap, no auto-restart, explicit shutdown reap,
//!   and duplicate-plugin-id rejection.
//! - [`persistent_remediation`]: CW10-11/14 cleanup evidence, secret redaction,
//!   drain timeout, strict shutdown-ack, and fail-fast health.
//! - [`provider_supervisor`]: CW10-02/09/11/14 one-shot lifecycle, first
//!   terminal, staged shutdown, and environment/secret containment.
//! - [`provider_worker`]: the descriptor to `OneShotRequest` to typed
//!   `ProviderMessage` path the background worker executes.
//! - [`startup_publication`]: CW10-01/13 composition from a real on-disk config
//!   into the single action registry, and what Help shows for it.
//! - [`recovery_zero_spawn`]: CW10-12, the offline command starting no provider.

#[path = "issue390/persistent_lifecycle.rs"]
mod persistent_lifecycle;
#[path = "issue390/persistent_remediation.rs"]
mod persistent_remediation;
#[path = "issue390/persistent_support.rs"]
mod persistent_support;
#[path = "issue390/provider_supervisor.rs"]
mod provider_supervisor;
#[path = "issue390/provider_worker.rs"]
mod provider_worker;
#[path = "issue390/recovery_zero_spawn.rs"]
mod recovery_zero_spawn;
#[path = "issue390/startup_publication.rs"]
mod startup_publication;
