//! Theme system for Jefe TUI.
//!
//! Provides JSON-based theme loading and management, inspired by
//! llxprt-code's theme system. The default theme is Green Screen.

pub mod definition;
pub mod loader;
pub mod manager;

pub use definition::{ResolvedColors, ThemeColors};
pub use manager::ThemeManager;
