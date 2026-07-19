//! Behavioral contracts for the first-agent tutorial's fail-closed GitHub fixture.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn shim() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/first-agent-tutorial-gh-shim.sh")
}

struct Fixture {
    _temp: TempDir,
    audit: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let audit = temp.path().join("audit.log");
        let state = temp.path().join("state");
        fs::write(&audit, "").unwrap_or_else(|error| panic!("write audit: {error}"));
        fs::write(&state, "open\n").unwrap_or_else(|error| panic!("write state: {error}"));
        Self {
            _temp: temp,
            audit,
            state,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new("sh")
            .arg(shim())
            .args(args)
            .env("TUTORIAL_GH_AUDIT", &self.audit)
            .env("TUTORIAL_GH_STATE", &self.state)
            .output()
            .unwrap_or_else(|error| panic!("run tutorial gh fixture: {error}"))
    }

    fn audit(&self) -> String {
        fs::read_to_string(&self.audit).unwrap_or_else(|error| panic!("read audit: {error}"))
    }
}

fn diagnostics(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn fixture_accepts_only_the_tutorial_auth_vector() {
    let fixture = Fixture::new();
    let accepted = fixture.run(&["auth", "status"]);
    assert!(
        accepted.status.success(),
        "allowlisted auth failed:\n{}",
        diagnostics(&accepted)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("tutorial-user"));
    assert!(fixture.audit().contains("ACCEPTED auth-status"));

    let rejected = fixture.run(&["auth", "status", "--show-token"]);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("fixture rejected"),
        "rejection diagnostic missing:\n{}",
        diagnostics(&rejected)
    );
    assert!(fixture.audit().contains("REJECTED"));
}

#[test]
fn fixture_rejects_mutated_graphql_flags() {
    let fixture = Fixture::new();
    let rejected = fixture.run(&[
        "api",
        "graphql",
        "--field",
        "query=unexpected",
        "-F",
        "owner=vybestack",
        "-F",
        "repo=llxprt-jefe",
        "-F",
        "number=352",
        "-F",
        "first=30",
    ]);

    assert!(!rejected.status.success());
    assert!(fixture.audit().contains("REJECTED"));
}

#[test]
fn fixture_records_the_squash_merge_and_returns_merged_detail() {
    let fixture = Fixture::new();
    let merged = fixture.run(&[
        "pr",
        "merge",
        "353",
        "--repo",
        "vybestack/llxprt-jefe",
        "--squash",
    ]);
    assert!(
        merged.status.success(),
        "fixture merge failed:\n{}",
        diagnostics(&merged)
    );
    assert_eq!(
        fs::read_to_string(&fixture.state)
            .unwrap_or_else(|error| panic!("read merged state: {error}")),
        "merged\n"
    );

    let detail = fixture.run(&[
        "pr",
        "view",
        "353",
        "--repo",
        "vybestack/llxprt-jefe",
        "--json",
        "number,title,state,mergedAt,author,createdAt,updatedAt,headRefName,headRefOid,baseRefName,isDraft,labels,assignees,milestone,body,url,reviewDecision,statusCheckRollup,reviews,mergeable,mergeStateStatus",
    ]);
    assert!(
        detail.status.success(),
        "merged detail failed:\n{}",
        diagnostics(&detail)
    );
    assert!(String::from_utf8_lossy(&detail.stdout).contains("\"state\":\"MERGED\""));
    let audit = fixture.audit();
    assert!(audit.contains("ACCEPTED pr-merge"));
    assert!(audit.contains("ACCEPTED pr-view"));
    assert!(!audit.contains("REJECTED"));
}
