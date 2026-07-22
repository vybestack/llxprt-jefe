use jefe::github::{
    parse_api_runs_json, parse_jobs_json, parse_runs_json, parse_single_run_json,
    parse_workflows_json,
};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

#[test]
fn test_parse_workflows_json() {
    let json = r#"[
        {
            "id": 12345,
            "name": "CI Build",
            "path": ".github/workflows/ci.yml",
            "state": "active"
        }
    ]"#;
    let workflows = parse_workflows_json(json).value_or_panic("should parse workflows");
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0].id, 12345);
    assert_eq!(workflows[0].name, "CI Build");
    assert_eq!(workflows[0].path, ".github/workflows/ci.yml");
    assert_eq!(workflows[0].state, "active");
}

#[test]
fn test_parse_runs_json() {
    let json = r#"[
        {
            "databaseId": 98765,
            "name": "fix bug",
            "headBranch": "main",
            "headSha": "abc123sha",
            "number": 42,
            "event": "push",
            "status": "completed",
            "conclusion": "success",
            "workflowName": "CI",
            "createdAt": "2026-07-06T10:00:00Z",
            "updatedAt": "2026-07-06T10:05:00Z"
        }
    ]"#;
    let runs = parse_runs_json(json).value_or_panic("should parse runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, 98765);
    assert_eq!(runs[0].name, "fix bug");
    assert_eq!(runs[0].head_branch, "main");
    assert_eq!(runs[0].head_sha, "abc123sha");
    assert_eq!(runs[0].run_number, 42);
    assert_eq!(runs[0].event, "push");
    assert_eq!(runs[0].status, jefe::domain::WorkflowRunStatus::Completed);
    assert_eq!(
        runs[0].conclusion,
        Some(jefe::domain::WorkflowRunConclusion::Success)
    );
    assert_eq!(runs[0].workflow_name, "CI");
}

#[test]
fn test_parse_api_runs_json() {
    let json = r#"{
        "total_count": 1,
        "workflow_runs": [
            {
                "id": 98765,
                "name": "CI",
                "display_title": "fix bug",
                "head_branch": "main",
                "head_sha": "abc123sha",
                "run_number": 42,
                "event": "push",
                "status": "in_progress",
                "conclusion": null,
                "created_at": "2026-07-06T10:00:00Z",
                "updated_at": "2026-07-06T10:05:00Z"
            }
        ]
    }"#;
    let (runs, total) = parse_api_runs_json(json).value_or_panic("should parse API runs");
    assert_eq!(total, 1);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, 98765);
    assert_eq!(runs[0].name, "fix bug");
    assert_eq!(runs[0].workflow_name, "CI");
    assert_eq!(runs[0].status, jefe::domain::WorkflowRunStatus::InProgress);
}

#[test]
fn test_parse_api_runs_json_null_head_branch() {
    let json = r#"{
        "total_count": 1,
        "workflow_runs": [
            {
                "id": 98765,
                "name": "CI",
                "display_title": "fix bug",
                "head_branch": null,
                "head_sha": "abc123sha",
                "run_number": 42,
                "event": "push",
                "status": "in_progress",
                "conclusion": null,
                "created_at": "2026-07-06T10:00:00Z",
                "updated_at": "2026-07-06T10:05:00Z"
            }
        ]
    }"#;
    let (runs, total) =
        parse_api_runs_json(json).value_or_panic("should parse API runs with null head_branch");
    assert_eq!(total, 1);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].head_branch, "");
}

#[test]
fn test_parse_jobs_json() {
    let json = r#"[
        {
            "databaseId": 11111,
            "name": "build",
            "status": "completed",
            "conclusion": "success",
            "steps": [
                {
                    "name": "Checkout",
                    "status": "completed",
                    "conclusion": "success",
                    "number": 1
                }
            ]
        }
    ]"#;
    let jobs = parse_jobs_json(json).value_or_panic("should parse jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, 11111);
    assert_eq!(jobs[0].name, "build");
    assert_eq!(jobs[0].steps.len(), 1);
    assert_eq!(jobs[0].steps[0].name, "Checkout");
    assert_eq!(jobs[0].steps[0].number, 1);
}

#[test]
fn test_parse_single_run_json() {
    let json = r#"{
        "databaseId": 98765,
        "name": "fix bug",
        "headBranch": "main",
        "headSha": "abc123sha",
        "number": 42,
        "event": "push",
        "status": "completed",
        "conclusion": "success",
        "workflowName": "CI",
        "createdAt": "2026-07-06T10:00:00Z",
        "updatedAt": "2026-07-06T10:05:00Z"
    }"#;
    let run = parse_single_run_json(json).value_or_panic("should parse single run");
    assert_eq!(run.id, 98765);
    assert_eq!(run.name, "fix bug");
    assert_eq!(run.head_branch, "main");
    assert_eq!(run.status, jefe::domain::WorkflowRunStatus::Completed);
    assert_eq!(
        run.conclusion,
        Some(jefe::domain::WorkflowRunConclusion::Success)
    );
}

#[test]
fn test_parse_runs_json_rejects_single_object() {
    let json = r#"{
        "databaseId": 98765,
        "name": "fix bug",
        "headBranch": "main",
        "headSha": "abc123sha",
        "number": 42,
        "event": "push",
        "status": "completed",
        "conclusion": "success",
        "workflowName": "CI",
        "createdAt": "2026-07-06T10:00:00Z",
        "updatedAt": "2026-07-06T10:05:00Z"
    }"#;
    assert!(
        parse_runs_json(json).is_err(),
        "parse_runs_json should reject single-object JSON"
    );
}
