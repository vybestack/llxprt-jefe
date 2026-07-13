#!/usr/bin/env bash
# Runner for the issue #265 real Linux tmux scenario.
#
# Sets up an isolated config directory with the CORRECT current Jefe
# persistence schema (settings.toml + state.json directly under the config
# dir), injects a fail-closed gh shim into PATH, runs the scenario via the
# real tmux driver, and verifies the exact accepted gh command sequence.
#
# The tmux-launched Jefe binary inherits this process's environment (the
# harness uses std::process::Command which inherits env by default, and
# Jefe's own gh subprocess calls inherit in turn), so PATH and GH_SHIM_AUDIT
# reach the shim.
#
# Usage:
#   scripts/issue265-run-scenario.sh [--keep-session]
#
# Requirements: tmux 3.x, cargo, the jefe binary (built if missing).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCENARIO="$PROJECT_ROOT/dev-docs/tmux-scenarios/issue265-linux-keys.json"
SHIM="$PROJECT_ROOT/scripts/issue265-gh-shim.sh"
FIXTURES="$PROJECT_ROOT/scripts/issue265-gh-shim-fixtures.sh"
ARTIFACT_DIR="$PROJECT_ROOT/target/tmux-harness/issue265"
CONFIG_DIR="$ARTIFACT_DIR/config"
AUDIT_FILE="$ARTIFACT_DIR/gh-audit.log"

command -v timeout >/dev/null 2>&1 || {
    echo "FATAL: timeout is required for the Linux tmux scenario" >&2
    exit 1
}

[[ -r "$SCENARIO" ]] || {
    echo "FATAL: scenario file is missing or not readable: $SCENARIO" >&2
    exit 1
}

[[ -r "$SHIM" ]] || {
    echo "FATAL: gh shim is missing or not readable: $SHIM" >&2
    exit 1
}

[[ -r "$FIXTURES" ]] || {
    echo "FATAL: shared shim fixtures are missing or not readable: $FIXTURES" >&2
    exit 1
}

# Optional extra args passed to the harness binary (array, properly quoted).
HARNESS_ARGS=()
if [[ "${1:-}" == "--keep-session" ]]; then
    HARNESS_ARGS+=("--keep-session")
fi

echo "== Issue #265 real tmux scenario =="

# Always build the jefe and harness binaries incrementally so the scenario
# validates current sources (a stale target/debug/jefe from a prior run would
# silently test old code). Keep output quiet unless something fails.
echo "Building jefe and jefe-tmux-harness (incremental)..."
(cd "$PROJECT_ROOT" && cargo build --bin jefe --bin jefe-tmux-harness 2>&1)

JEFE_BIN="$PROJECT_ROOT/target/debug/jefe"
HARNESS_BIN="$PROJECT_ROOT/target/debug/jefe-tmux-harness"

# Safe recreate of the isolated config directory (only under target/).
if [[ "$CONFIG_DIR" == "$PROJECT_ROOT/target/"* ]]; then
    rm -rf "$CONFIG_DIR"
fi
mkdir -p "$CONFIG_DIR"

# Seed settings.toml (TOML format, schema version 1).
cat > "$CONFIG_DIR/settings.toml" <<'EOF'
schema_version = 1
theme = "green-screen"
override_agent_theme = false
EOF

# Seed state.json with a single repository that has a github_repo slug so
# Issues mode can fetch issues. NO agents are seeded so that pressing `S`
# in Issue Detail yields the "No agents available" notice.
#
# The repository id is "repo-265" and github_repo is "owner/repo-265".
cat > "$CONFIG_DIR/state.json" <<'EOF'
{
  "schema_version": 1,
  "repositories": [
    {
      "id": "repo-265",
      "name": "repo-265",
      "slug": "repo-265",
      "base_dir": "/tmp/jefe-issue265-repo",
      "default_profile": "",
      "default_code_puppy_model": "",
      "github_repo": "owner/repo-265",
      "remote": { "enabled": false, "host": "", "user": "", "port": null },
      "issue_base_prompt": "",
      "default_agent_kind": "llxprt",
      "agent_ids": []
    }
  ],
  "agents": [],
  "selected_repository_index": 0,
  "selected_agent_index": null,
  "hide_idle_repositories": false,
  "last_selected_agent_by_repo": [],
  "pane_focus": "",
  "terminal_focused": false,
  "user_preferences": {}
}
EOF

# Clean previous audit.
rm -f "$AUDIT_FILE"

# Build an isolated PATH directory with the gh shim.
SHIM_DIR="$ARTIFACT_DIR/shim-bin"
rm -rf "$SHIM_DIR"
mkdir -p "$SHIM_DIR"
cp "$SHIM" "$SHIM_DIR/gh"
cp "$FIXTURES" "$SHIM_DIR/issue265-gh-shim-fixtures.sh"
chmod +x "$SHIM_DIR/gh"

export GH_SHIM_AUDIT="$AUDIT_FILE"

echo "Running scenario..."
cd "$PROJECT_ROOT"

timeout 180s env \
    PATH="$SHIM_DIR:$PATH" \
    GH_SHIM_AUDIT="$AUDIT_FILE" \
    "$HARNESS_BIN" \
    --scenario "$SCENARIO" \
    --jefe-bin "$JEFE_BIN" \
    --config "$CONFIG_DIR" \
    --out-dir "$ARTIFACT_DIR" \
    --session "jefe-issue265" \
    "${HARNESS_ARGS[@]}"

echo ""
echo "== Verifying gh audit =="

# The audit file MUST exist and be non-empty: the scenario issues real gh
# read commands (search, issue-view, comments, search-refresh). An absent or
# empty audit means the shim was never invoked — a configuration failure.
if [[ ! -s "$AUDIT_FILE" ]]; then
    echo "FAIL: gh audit is missing or empty (shim was never invoked)."
    echo "  audit file: $AUDIT_FILE"
    exit 1
fi

echo "Audit log:"
cat "$AUDIT_FILE"

# ─── Reject any REJECTED or mutation record ──────────────────────────────
#
# A REJECTED record means the shim saw an unexpected command — a fail-closed
# violation. Any mutation keyword means a write slipped through (should be
# impossible given exact matching, but belt-and-suspenders).
if grep -qiE 'REJECTED' "$AUDIT_FILE"; then
    echo ""
    echo "FAIL: rejected command detected in audit:"
    grep -i 'REJECTED' "$AUDIT_FILE"
    exit 1
fi

if grep -qiE 'mutation|POST|PATCH|DELETE|createIssue|addComment|closeIssue|deleteIssue|issue create|issue close|issue delete|issue comment|issue edit' "$AUDIT_FILE"; then
    echo ""
    echo "FAIL: gh mutation command detected in audit:"
    grep -iE 'mutation|POST|PATCH|DELETE|createIssue|addComment|closeIssue|deleteIssue|issue create|issue close|issue delete|issue comment|issue edit' "$AUDIT_FILE"
    exit 1
fi

# ─── Validate exact accepted command sequence ────────────────────────────
#
# The scenario must produce exactly four ACCEPTED issue-read operations in
# this order:
#   1. search         — initial issue list load
#   2. issue-view     — issue detail load
#   3. comments       — issue comments load
#   4. search         — list refresh on return
#
# An optional `auth-status` ACCEPTED call may appear (if production invokes
# `gh auth status`); it is excluded from the four-operation sequence check
# because it is not an issue read.
#
# Extract ACCEPTED operation labels (normalize whitespace, ignore timestamps).
# Filter out auth-status so the four-operation sequence check is exact.
#
# The `grep -v '^auth-status$'` returns exit 1 when only auth-status records
# exist (no issue-read operations). Under `set -o pipefail` that would abort
# the script before reaching the explicit `FAIL:` branch below, so `|| true`
# lets an empty result flow through to the no-operations check.
accepted_seq=$(grep -oE 'ACCEPTED [A-Za-z0-9_-]+' "$AUDIT_FILE" \
    | sed 's/ACCEPTED //' \
    | grep -v '^auth-status$' || true)

if [[ -z "$accepted_seq" ]]; then
    echo "FAIL: no ACCEPTED issue-read operations in audit."
    exit 1
fi

# Count accepted issue-read operations.
accepted_count=$(echo "$accepted_seq" | wc -l | tr -d ' ')

echo ""
echo "Accepted issue-read sequence ($accepted_count ops):"
echo "$accepted_seq"

# The expected exact four-operation sequence.
expected_seq="search
issue-view
comments
search"

if [[ "$accepted_seq" != "$expected_seq" ]]; then
    echo ""
    echo "FAIL: accepted issue-read sequence does not match expected four operations."
    echo "  expected:"
    echo "$expected_seq" | sed 's/^/    /'
    echo "  actual:"
    echo "$accepted_seq" | sed 's/^/    /'
    exit 1
fi

# Report any auth-status calls separately for transparency.
auth_count=$(grep -c 'ACCEPTED auth-status' "$AUDIT_FILE" || true)
if [[ "$auth_count" -gt 0 ]]; then
    echo "(auth-status calls: $auth_count — excluded from four-operation check)"
fi

echo ""
echo "PASS: exact accepted command sequence verified (search, issue-view, comments, search)."
echo "PASS: no mutations or rejected commands."
