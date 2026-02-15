//! Core data models for the Jefe TUI application.
//!
//! This module defines the primary data structures used throughout
//! the application, including agents, tasks, projects, and output.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The current execution status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Agent is actively running.
    Running,
    /// Agent has completed its task successfully.
    Completed,
    /// Agent encountered an error.
    Errored,
    /// Agent is waiting for input or resources.
    Waiting,
    /// Agent has been paused by the user.
    Paused,
    /// Agent is queued to start.
    Queued,
    /// Agent process was terminated/exited and PTY is no longer alive.
    Dead,
}

/// The completion status of a todo item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    /// Todo is not yet started.
    Pending,
    /// Todo is currently being worked on.
    InProgress,
    /// Todo has been completed.
    Completed,
}

/// A single todo item within a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// The description of what needs to be done.
    pub content: String,
    /// The current completion status.
    pub status: TodoStatus,
}

/// An AI agent instance (llxprt-code) working in a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier for this agent.
    pub id: Uuid,
    /// Human-readable display ID (e.g., "#1872").
    pub display_id: String,
    /// Short name for this agent (used for dir slug and display).
    pub name: String,
    /// Longer description of what this agent is doing.
    pub description: String,
    /// Working directory for this agent.
    pub work_dir: String,
    /// The profile configuration (e.g., "default").
    pub profile: String,
    /// The execution mode (e.g., "--yolo").
    pub mode: String,
    /// Stable PTY session slot in `PtyManager`.
    ///
    /// `None` means no PTY session is currently allocated.
    pub pty_slot: Option<usize>,
    /// Current execution status.
    pub status: AgentStatus,
    /// When this agent started running.
    pub started_at: DateTime<Utc>,
    /// Total input tokens consumed.
    pub token_in: u64,
    /// Total output tokens generated.
    pub token_out: u64,
    /// Estimated cost in USD.
    pub cost_usd: f64,
    /// List of todos for this agent's current work.
    pub todos: Vec<TodoItem>,
    /// Recent output lines from the agent.
    pub recent_output: Vec<OutputLine>,
    /// Total elapsed time in seconds.
    pub elapsed_secs: u64,
}

/// The kind of output line from an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputKind {
    /// Regular text output.
    Text,
    /// A tool call invocation.
    ToolCall,
}

/// The execution status of a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool call is currently executing.
    InProgress,
    /// Tool call completed successfully.
    Completed,
    /// Tool call failed with an error.
    Failed,
}

/// A single line of output from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputLine {
    /// The type of output.
    pub kind: OutputKind,
    /// The actual content/text.
    pub content: String,
    /// Tool execution status (only for tool calls).
    pub tool_status: Option<ToolStatus>,
}

/// A repository (codebase) being managed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    /// Display name of the repository.
    pub name: String,
    /// URL-safe slug identifier.
    pub slug: String,
    /// Base directory path for this repository.
    pub base_dir: String,
    /// Default llxprt-code profile for new agents in this repo.
    pub default_profile: String,
    /// All agents working on this repository.
    pub agents: Vec<Agent>,
}

/// Derive a working directory from a repo base dir and agent name.
/// Lowercases, replaces spaces with dashes, strips non-alphanumeric chars except dash.
pub fn agent_work_dir(repo_base_dir: &str, name: &str) -> String {
    let slug = name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '/')
        .collect::<String>();
    format!("{}/{}", repo_base_dir.trim_end_matches('/'), slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_equality() {
        assert_eq!(AgentStatus::Running, AgentStatus::Running);
        assert_ne!(AgentStatus::Running, AgentStatus::Completed);
    }

    #[test]
    fn test_todo_status_equality() {
        assert_eq!(TodoStatus::Pending, TodoStatus::Pending);
        assert_ne!(TodoStatus::Pending, TodoStatus::InProgress);
    }

    #[test]
    fn test_output_kind_equality() {
        assert_eq!(OutputKind::Text, OutputKind::Text);
        assert_ne!(OutputKind::Text, OutputKind::ToolCall);
    }

    #[test]
    fn test_tool_status_equality() {
        assert_eq!(ToolStatus::Completed, ToolStatus::Completed);
        assert_ne!(ToolStatus::Completed, ToolStatus::Failed);
    }

    #[test]
    fn test_todo_item_creation() {
        let todo = TodoItem {
            content: "Test todo".to_string(),
            status: TodoStatus::Pending,
        };
        assert_eq!(todo.content, "Test todo");
        assert_eq!(todo.status, TodoStatus::Pending);
    }

    #[test]
    fn test_output_line_with_tool_status() {
        let output = OutputLine {
            kind: OutputKind::ToolCall,
            content: "llxprt_read_file".to_string(),
            tool_status: Some(ToolStatus::Completed),
        };
        assert_eq!(output.kind, OutputKind::ToolCall);
        assert_eq!(output.tool_status, Some(ToolStatus::Completed));
    }

    #[test]
    fn test_output_line_text_no_tool_status() {
        let output = OutputLine {
            kind: OutputKind::Text,
            content: "Processing request...".to_string(),
            tool_status: None,
        };
        assert_eq!(output.kind, OutputKind::Text);
        assert_eq!(output.tool_status, None);
    }

    #[test]
    fn test_repository_creation() {
        let repository = Repository {
            name: "test-repository".to_string(),
            slug: "test-repository".to_string(),
            base_dir: "/home/user/test-repository".to_string(),
            default_profile: "default".to_string(),
            agents: vec![],
        };
        assert_eq!(repository.name, "test-repository");
        assert_eq!(repository.agents.len(), 0);
    }

    #[test]
    fn test_agent_work_dir() {
        assert_eq!(
            agent_work_dir("/Users/acoliver/projects/llxprt-code", "Fix ACP socket timeout"),
            "/Users/acoliver/projects/llxprt-code/fix-acp-socket-timeout"
        );
        assert_eq!(
            agent_work_dir("/tmp/repo/", "Add retry on 429"),
            "/tmp/repo/add-retry-on-429"
        );
        assert_eq!(
            agent_work_dir("/base", "Test@#$%Special!!Chars"),
            "/base/testspecialchars"
        );
    }
}
