//! Core contract integration tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P04
//! @requirement REQ-TECH-002

#[cfg(unix)]
mod clippy_allow_policy;
mod domain_state_contracts;
mod message_bus_contracts;
mod ocr_workflow_contracts;
mod persistence_theme_contracts;
mod tmux_harness_docs_contracts;
mod visibility_filter_contracts;
