//! xtask — cross-platform developer and quality automation for Jefe (issue #459).
//!
//! Replaces GNU Make, Bash, and embedded Python repository automation with a
//! single Rust task runner. Invoked via the `cargo xtask` alias defined in
//! `.cargo/config.toml`, so contributors never need an external
//! `cargo-xtask` installation.

use std::process::ExitCode;

use xtask::cli;

fn main() -> ExitCode {
    // Skip the program name; the rest is the xtask command + args.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    cli::run(&argv)
}
