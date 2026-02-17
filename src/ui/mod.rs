//! UI components and screens for the Jefe TUI.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-010
//!
//! Built with iocraft - React-like declarative components for Rust terminals.
//! This module reuses toy1 UI composition patterns while consuming rebuilt
//! core contracts from domain/state/runtime/theme layers.

pub mod components;
pub mod modals;
pub mod screens;

// Re-export commonly used types
pub use components::{AgentList, KeybindBar, Preview, Sidebar, StatusBar, TerminalView};
pub use modals::{ConfirmModal, HelpModal};
pub use screens::{Dashboard, NewAgentForm, NewRepositoryForm, SplitScreen};
