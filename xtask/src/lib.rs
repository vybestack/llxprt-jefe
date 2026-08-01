//! xtask library surface (issue #459).
//!
//! Exposed so integration tests can assert command plans and policy behavior
//! without spawning the xtask binary. The binary in `main.rs` is a thin entry
//! point over these same modules.

#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

pub mod architecture;
pub mod cli;
pub mod clippy_policy;
pub mod process;
pub mod source_size;
pub mod toolchain;
pub mod windows_coverage;
