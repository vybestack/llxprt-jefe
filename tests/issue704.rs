//! Issue #704 — one atomic workbench candidate and its startup commit.
//!
//! Slice S1 owns the process-free static phase: exact selected-package
//! outcomes, provider requiredness, and static refusal before any process or
//! durable write (CWR1-00, CWR1-02). Slice S2 owns the required-provider
//! startup transaction: all-candidates preparation before any spawn,
//! deterministic plugin-id order, typed preparation failures, and all-or-
//! nothing publication with rollback reaping (CWR1-00, CWR1-03, CWR1-04,
//! CWR1-05). Later slices add the single commit and the consumer cutover;
//! their suites join this binary as they land.

#[path = "issue704/support.rs"]
mod support;

#[path = "issue704/selection.rs"]
mod selection;

#[path = "issue704/static_candidate.rs"]
mod static_candidate;

#[path = "issue704/transaction_support.rs"]
mod transaction_support;

#[path = "issue704/transaction.rs"]
mod transaction;

#[path = "issue704/commit.rs"]
mod commit;

#[path = "issue704/consumer_cutover.rs"]
mod consumer_cutover;

#[path = "issue704/startup_failure_diagnostics.rs"]
mod startup_failure_diagnostics;
