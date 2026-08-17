//! Integration test entry point.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P04
//! @plan PLAN-20260216-FIRSTVERSION-V1.P07
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @plan PLAN-20260216-FIRSTVERSION-V1.P13
#[path = "selection/model.rs"]
mod selection_model;

#[path = "common/app_state.rs"]
mod common_app_state;
mod core;
mod e2e;
mod runtime;
mod support;
mod ui;
