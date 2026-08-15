//! Issue #704 — one atomic workbench candidate and its startup commit.
//!
//! Slice S1 owns the process-free static phase: exact selected-package
//! outcomes, provider requiredness, and static refusal before any process or
//! durable write (CWR1-00, CWR1-02). Later slices add the required-provider
//! transaction, the single commit, and the consumer cutover; their suites
//! join this binary as they land.

#[path = "issue704/support.rs"]
mod support;

#[path = "issue704/selection.rs"]
mod selection;

#[path = "issue704/static_candidate.rs"]
mod static_candidate;
