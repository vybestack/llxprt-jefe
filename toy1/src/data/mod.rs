//! Data layer for the Jefe TUI application.
//!
//! This module contains the core data models, mock data generation,
//! and data access patterns for the application.

pub mod mock;
pub mod models;

pub use models::{Agent, AgentStatus, OutputKind, OutputLine, Repository, TodoItem, TodoStatus, ToolStatus};
