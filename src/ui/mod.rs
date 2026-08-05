//! UI components and screens for the Jefe TUI.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-010
//!
//! Built with iocraft - React-like declarative components for Rust terminals.
//! This module reuses toy1 UI composition patterns while consuming rebuilt
//! core contracts from domain/state/runtime/theme layers.

pub mod components;
mod dashboard_filter_indicator;
pub mod modals;
pub mod orchestration;
pub mod screens;
pub mod util;

// Re-export commonly used types
pub use components::{KeybindBar, Preview, SelectableList, Sidebar, StatusBar, TerminalView};
pub use dashboard_filter_indicator::dashboard_filter_indicator;
pub use modals::{AuthModal, ConfirmModal, HelpModal};
pub use screens::{
    ActionsScreen, Dashboard, GeneratedAgentForm, NewAgentForm, NewRepositoryForm, SplitScreen,
    WorkflowDispatchForm,
};
pub use util::{CARET_CHAR, ELLIPSIS, text_with_caret, truncate_with_ellipsis};
