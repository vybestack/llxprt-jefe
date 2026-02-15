//! Mock data generator for development and testing.
//!
//! This module provides realistic-looking fake data for the TUI
//! to render during development and prototyping.

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::models::{
    Agent, AgentStatus, OutputKind, OutputLine, Repository, TodoItem, TodoStatus, ToolStatus,
};

/// Generates realistic mock data for development and testing.
///
/// Returns a collection of repositories with various agents in different states,
/// complete with todos and output lines.
#[must_use]
pub fn generate_mock_data() -> Vec<Repository> {
    vec![
        create_llxprt_code_repo(),
        create_starflight_tls_repo(),
        create_gable_work_repo(),
        create_mariadb_cli_repo(),
    ]
}

fn create_llxprt_code_repo() -> Repository {
    let base_dir = "/Users/acoliver/projects/llxprt-code";

    Repository {
        name: "llxprt-code".to_owned(),
        slug: "llxprt-code".to_owned(),
        base_dir: base_dir.to_owned(),
        default_profile: "default".to_owned(),
        agents: vec![
            create_acp_socket_agent(base_dir),
            create_refactor_prompt_agent(base_dir),
            create_retry_429_agent(base_dir),
        ],
    }
}

fn create_acp_socket_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#1872".to_owned(),
        name: "Fix ACP socket timeout".to_owned(),
        description: "Implementing timeout handling for ACP socket connections to prevent hangs".to_owned(),
        work_dir: format!("{}/fix-acp-socket-timeout", base_dir),
        profile: "default".to_owned(),
        mode: "--yolo".to_owned(),
        pty_slot: None,
        status: AgentStatus::Running,
        started_at: now - Duration::minutes(42),
        token_in: 125_840,
        token_out: 43_210,
        cost_usd: 2.47,
        todos: vec![
            TodoItem {
                content: "Read src/acp/connection.rs".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Check settings.rs for timeout configuration".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Update socket creation with timeout parameter".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Add timeout validation tests".to_owned(),
                status: TodoStatus::InProgress,
            },
            TodoItem {
                content: "Update documentation".to_owned(),
                status: TodoStatus::Pending,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::Text,
                content: "Analyzing the ACP connection module...".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_read_file(src/acp/connection.rs)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_search_file_content(pattern: timeout)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Found the issue: socket is created with hardcoded 30s timeout instead of reading from config.".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_replace(src/acp/connection.rs)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Now adding tests to verify timeout behavior...".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_write_file(tests/acp_timeout_test.rs)".to_owned(),
                tool_status: Some(ToolStatus::InProgress),
            },
        ],
        elapsed_secs: 42 * 60,
    }
}

fn create_refactor_prompt_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#1899".to_owned(),
        name: "Refactor prompt handler".to_owned(),
        description: "Restructuring the prompt validation and processing pipeline".to_owned(),
        work_dir: format!("{}/refactor-prompt-handler", base_dir),
        profile: "default".to_owned(),
        mode: "--auto-approve".to_owned(),
        pty_slot: None,
        status: AgentStatus::Running,
        started_at: now - Duration::minutes(75),
        token_in: 203_456,
        token_out: 89_123,
        cost_usd: 4.12,
        todos: vec![
            TodoItem {
                content: "Understand current prompt handler implementation".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Research new streaming API documentation".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Extract shared logic into utility functions".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Implement streaming API integration".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Add comprehensive error handling".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Run existing test suite".to_owned(),
                status: TodoStatus::Completed,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_run_shell_command(cargo test prompt)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "All tests passing! The refactoring is complete.".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Summary of changes:".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "- Migrated from blocking API to streaming API".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "- Added Result<T, PromptError> throughout".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "- Extracted 3 utility functions to reduce duplication".to_owned(),
                tool_status: None,
            },
        ],
        elapsed_secs: 75 * 60,
    }
}

fn create_retry_429_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#1905".to_owned(),
        name: "Add retry on 429".to_owned(),
        description: "Implementing exponential backoff retry logic for rate-limited API requests".to_owned(),
        work_dir: format!("{}/add-retry-on-429", base_dir),
        profile: "default".to_owned(),
        mode: "--yolo".to_owned(),
        pty_slot: None,
        status: AgentStatus::Completed,
        started_at: now - Duration::minutes(28),
        token_in: 87_234,
        token_out: 31_567,
        cost_usd: 1.54,
        todos: vec![
            TodoItem {
                content: "Find API client code".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Implement exponential backoff utility".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Add retry logic to API calls".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Write tests for retry behavior".to_owned(),
                status: TodoStatus::Completed,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::Text,
                content: "Task completed successfully!".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Added retry logic with exponential backoff (base 2s, max 60s).".to_owned(),
                tool_status: None,
            },
        ],
        elapsed_secs: 28 * 60,
    }
}

fn create_starflight_tls_repo() -> Repository {
    let base_dir = "/Users/acoliver/projects/starflight-tls";

    Repository {
        name: "starflight-tls".to_owned(),
        slug: "starflight-tls".to_owned(),
        base_dir: base_dir.to_owned(),
        default_profile: "go-expert".to_owned(),
        agents: vec![
            create_tls_renegotiation_agent(base_dir),
            create_cert_rotation_agent(base_dir),
        ],
    }
}

fn create_tls_renegotiation_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#42".to_owned(),
        name: "TLS renegotiation fix".to_owned(),
        description: "Fixing TLS renegotiation handling in secure connections".to_owned(),
        work_dir: format!("{}/tls-renegotiation-fix", base_dir),
        profile: "go-expert".to_owned(),
        mode: "--auto-approve".to_owned(),
        pty_slot: None,
        status: AgentStatus::Running,
        started_at: now - Duration::minutes(18),
        token_in: 54_321,
        token_out: 23_456,
        cost_usd: 0.89,
        todos: vec![
            TodoItem {
                content: "Review TLS 1.3 renegotiation specs".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Examine current handshake implementation".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Implement client-initiated renegotiation handler".to_owned(),
                status: TodoStatus::InProgress,
            },
            TodoItem {
                content: "Add integration tests".to_owned(),
                status: TodoStatus::Pending,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_read_file(lib/tls/handshake.go)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Analyzing the handshake state machine...".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_codesearch(TLS renegotiation golang)".to_owned(),
                tool_status: Some(ToolStatus::InProgress),
            },
        ],
        elapsed_secs: 18 * 60,
    }
}

fn create_cert_rotation_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#38".to_owned(),
        name: "Cert rotation handler".to_owned(),
        description: "Building automatic certificate rotation and renewal system".to_owned(),
        work_dir: format!("{}/cert-rotation-handler", base_dir),
        profile: "go-expert".to_owned(),
        mode: "--yolo".to_owned(),
        pty_slot: None,
        status: AgentStatus::Completed,
        started_at: now - Duration::minutes(45),
        token_in: 112_890,
        token_out: 45_678,
        cost_usd: 1.98,
        todos: vec![
            TodoItem {
                content: "Implement file watcher for cert directory".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Add cert reload logic".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Handle reload errors gracefully".to_owned(),
                status: TodoStatus::Completed,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::Text,
                content: "Certificate rotation handler complete!".to_owned(),
                tool_status: None,
            },
        ],
        elapsed_secs: 45 * 60,
    }
}

fn create_gable_work_repo() -> Repository {
    let base_dir = "/Users/acoliver/projects/gable-work";

    Repository {
        name: "gable-work".to_owned(),
        slug: "gable-work".to_owned(),
        base_dir: base_dir.to_owned(),
        default_profile: "default".to_owned(),
        agents: vec![create_api_migration_agent(base_dir)],
    }
}

#[allow(clippy::too_many_lines)]
fn create_api_migration_agent(base_dir: &str) -> Agent {
    let now = Utc::now();

    Agent {
        id: Uuid::new_v4(),
        display_id: "#156".to_owned(),
        name: "API migration v3".to_owned(),
        description: "Migrating REST API endpoints from v2 to v3 schema".to_owned(),
        work_dir: format!("{}/api-migration-v3", base_dir),
        profile: "default".to_owned(),
        mode: "--auto-approve".to_owned(),
        pty_slot: None,
        status: AgentStatus::Running,
        started_at: now - Duration::hours(2),
        token_in: 456_789,
        token_out: 178_234,
        cost_usd: 8.92,
        todos: vec![
            TodoItem {
                content: "Audit all v2 endpoints".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Create v3 schema definitions".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Migrate GET /users endpoint".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Migrate POST /users endpoint".to_owned(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Migrate /teams endpoints".to_owned(),
                status: TodoStatus::InProgress,
            },
            TodoItem {
                content: "Migrate /projects endpoints".to_owned(),
                status: TodoStatus::Pending,
            },
            TodoItem {
                content: "Add backward compatibility layer".to_owned(),
                status: TodoStatus::Pending,
            },
            TodoItem {
                content: "Update API documentation".to_owned(),
                status: TodoStatus::Pending,
            },
        ],
        recent_output: vec![
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_read_file(api/v2/teams.py)".to_owned(),
                tool_status: Some(ToolStatus::Completed),
            },
            OutputLine {
                kind: OutputKind::Text,
                content: "Analyzing teams endpoint structure...".to_owned(),
                tool_status: None,
            },
            OutputLine {
                kind: OutputKind::ToolCall,
                content: "llxprt_write_file(api/v3/teams.py)".to_owned(),
                tool_status: Some(ToolStatus::InProgress),
            },
        ],
        elapsed_secs: 2 * 60 * 60,
    }
}

fn create_mariadb_cli_repo() -> Repository {
    Repository {
        name: "mariadb-cli".to_owned(),
        slug: "mariadb-cli".to_owned(),
        base_dir: "/Users/acoliver/projects/mariadb-cli".to_owned(),
        default_profile: "default".to_owned(),
        agents: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mock_data_returns_repositories() {
        let repositories = generate_mock_data();
        assert_eq!(repositories.len(), 4);
    }

    #[test]
    fn test_llxprt_code_repository_structure() {
        let repositories = generate_mock_data();
        let llxprt = &repositories[0];

        assert_eq!(llxprt.name, "llxprt-code");
        assert_eq!(llxprt.slug, "llxprt-code");
        assert_eq!(llxprt.agents.len(), 3);
    }

    #[test]
    fn test_acp_socket_agent_has_correct_status() {
        let repositories = generate_mock_data();
        let agent = &repositories[0].agents[0];

        assert_eq!(agent.display_id, "#1872");
        assert_eq!(agent.status, AgentStatus::Running);
    }

    #[test]
    fn test_completed_agent_structure() {
        let repositories = generate_mock_data();
        let agent = &repositories[0].agents[2];

        assert_eq!(agent.display_id, "#1905");
        assert_eq!(agent.status, AgentStatus::Completed);
    }

    #[test]
    fn test_mariadb_cli_has_no_agents() {
        let repositories = generate_mock_data();
        let mariadb = &repositories[3];

        assert_eq!(mariadb.name, "mariadb-cli");
        assert_eq!(mariadb.agents.len(), 0);
    }

    #[test]
    fn test_agents_have_output_lines() {
        let repositories = generate_mock_data();
        let agent = &repositories[0].agents[0];

        assert!(!agent.recent_output.is_empty());

        let has_text = agent
            .recent_output
            .iter()
            .any(|line| line.kind == OutputKind::Text);
        let has_tool_call = agent
            .recent_output
            .iter()
            .any(|line| line.kind == OutputKind::ToolCall);

        assert!(has_text);
        assert!(has_tool_call);
    }

    #[test]
    fn test_todos_have_different_statuses() {
        let repositories = generate_mock_data();
        let agent = &repositories[0].agents[0];

        let completed = agent
            .todos
            .iter()
            .any(|t| t.status == TodoStatus::Completed);
        let in_progress = agent
            .todos
            .iter()
            .any(|t| t.status == TodoStatus::InProgress);
        let pending = agent.todos.iter().any(|t| t.status == TodoStatus::Pending);

        assert!(completed);
        assert!(in_progress);
        assert!(pending);
    }
}
