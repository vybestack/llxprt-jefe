#!/usr/bin/env bash
# Runner for the issue #230 real Linux tmux scenario.
#
# Sets up an isolated environment (temp HOME, config, state, PATH) seeded
# with a repository and two eligible local agents (one LLxprt with a
# configured profile, one Code Puppy with an empty model → default). Creates
# real local Git worktrees on known branches and makes one genuinely dirty
# with a non-owned change. Injects a fail-closed gh shim and executable
# availability shims (llxprt, code-puppy) into PATH. Runs the scenario via
# the real tmux driver and verifies the exact accepted gh command sequence.
#
# The tmux-launched Jefe binary inherits this process's environment so the
# injected PATH, HOME, and GH_SHIM_AUDIT reach the child processes.
#
# Usage:
#   scripts/issue230-run-scenario.sh [--keep-session]
#
# Requirements: tmux 3.x, cargo, the jefe binary (built if missing), git.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
SCENARIO="$PROJECT_ROOT/dev-docs/tmux-scenarios/send-to-agent-details.json"
SHIM="$PROJECT_ROOT/scripts/issue230-gh-shim.sh"
FIXTURES="$PROJECT_ROOT/scripts/issue230-gh-shim-fixtures.sh"
ARTIFACT_DIR="$PROJECT_ROOT/target/tmux-harness/issue230-$$"
CONFIG_DIR="$ARTIFACT_DIR/config"
SHIM_DIR="$ARTIFACT_DIR/shim-bin"
REPO_DIR="$ARTIFACT_DIR/repo"
WORKTREE_LLX="$ARTIFACT_DIR/wt-llxprt-alpha"
WORKTREE_PUP="$ARTIFACT_DIR/wt-codepuppy-beta"
AUDIT_FILE="$ARTIFACT_DIR/gh-audit.log"

command -v timeout >/dev/null 2>&1 || {
    echo "FATAL: timeout is required for the Linux tmux scenario" >&2
    exit 1
}
command -v realpath >/dev/null 2>&1 || {
    echo "FATAL: realpath is required for safe scenario cleanup" >&2
    exit 1
}
command -v cargo >/dev/null 2>&1 || {
    echo "FATAL: cargo is required for the Linux tmux scenario" >&2
    exit 1
}
command -v tmux >/dev/null 2>&1 || {
    echo "FATAL: tmux is required for the Linux tmux scenario" >&2
    exit 1
}
command -v git >/dev/null 2>&1 || {
    echo "FATAL: git is required for the Linux tmux scenario" >&2
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

HARNESS_ARGS=()
KEEP_SESSION=false
if [[ "${1:-}" == "--keep-session" ]]; then
    HARNESS_ARGS+=("--keep-session")
    KEEP_SESSION=true
fi
SESSION_NAME="jefe-issue230-$$"

cleanup_session() {
    if [[ "$KEEP_SESSION" == false ]]; then
        tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    fi
}
trap cleanup_session EXIT

echo "== Issue #230 real tmux scenario =="

echo "Building jefe and jefe-tmux-harness (incremental)..."
(cd "$PROJECT_ROOT" && cargo build --bin jefe --bin jefe-tmux-harness 2>&1)

JEFE_BIN="$PROJECT_ROOT/target/debug/jefe"
HARNESS_BIN="$PROJECT_ROOT/target/debug/jefe-tmux-harness"
[[ -x "$JEFE_BIN" ]] || {
    echo "FATAL: jefe binary is missing or not executable: $JEFE_BIN" >&2
    exit 1
}
[[ -x "$HARNESS_BIN" ]] || {
    echo "FATAL: harness binary is missing or not executable: $HARNESS_BIN" >&2
    exit 1
}

# ── Require every mutable scenario path to stay under target/ ────────────
TARGET_ROOT_REAL="$(realpath -m "$PROJECT_ROOT/target")"
require_target_descendant() {
    local candidate="$1"
    local candidate_real
    candidate_real="$(realpath -m "$candidate")"
    if [[ "$candidate_real" != "$TARGET_ROOT_REAL/"* ]]; then
        echo "FATAL: refusing scenario path outside target: $candidate_real" >&2
        exit 1
    fi
}
require_target_descendant "$ARTIFACT_DIR"
require_target_descendant "$CONFIG_DIR"
require_target_descendant "$SHIM_DIR"
require_target_descendant "$REPO_DIR"
require_target_descendant "$WORKTREE_LLX"
require_target_descendant "$WORKTREE_PUP"

rm -rf "$CONFIG_DIR" "$SHIM_DIR" "$REPO_DIR" "$WORKTREE_LLX" "$WORKTREE_PUP"
mkdir -p "$CONFIG_DIR" "$REPO_DIR"

# ── Seed settings.toml ──────────────────────────────────────────────────
cat > "$CONFIG_DIR/settings.toml" <<'EOF'
schema_version = 1
theme = "green-screen"
override_agent_theme = false
EOF

# ── Create real local Git worktrees on known branches ───────────────────
#
# Each worktree is an independent git repo (not a shared-object worktree) so
# that the branch name is deterministic and the dirty state is independently
# controllable.

# LLxprt agent worktree: branch "main", will be made dirty.
mkdir -p "$WORKTREE_LLX"
git -C "$WORKTREE_LLX" init -q -b main
git -C "$WORKTREE_LLX" config user.email "test@example.com"
git -C "$WORKTREE_LLX" config user.name "Test User"
echo "# LLxprt worktree" > "$WORKTREE_LLX/README.md"
git -C "$WORKTREE_LLX" add README.md
git -C "$WORKTREE_LLX" commit -q -m "initial commit"

# Code Puppy agent worktree: branch "feature", will stay clean.
mkdir -p "$WORKTREE_PUP"
git -C "$WORKTREE_PUP" init -q -b feature
git -C "$WORKTREE_PUP" config user.email "test@example.com"
git -C "$WORKTREE_PUP" config user.name "Test User"
echo "# Code Puppy worktree" > "$WORKTREE_PUP/README.md"
git -C "$WORKTREE_PUP" add README.md
git -C "$WORKTREE_PUP" commit -q -m "initial commit"

# Make the LLxprt worktree genuinely dirty with a NON-owned change.
echo "uncommitted change" > "$WORKTREE_LLX/src-change.txt"

# ── Seed state.json with repository + two eligible local agents ─────────
#
# Agent "alpha": LLxprt, profile "ops", worktree on branch "main" (dirty).
# Agent "beta": Code Puppy, empty model (→ default), worktree on branch
# "feature" (clean).
cat > "$CONFIG_DIR/state.json" <<EOF
{
  "schema_version": 1,
  "repositories": [
    {
      "id": "repo-230",
      "name": "repo-230",
      "slug": "repo-230",
      "base_dir": "$REPO_DIR",
      "default_profile": "",
      "default_code_puppy_model": "",
      "github_repo": "owner/repo-230",
      "github_issue_pr_repo": "",
      "remote": { "enabled": false, "login_user": "", "host": "", "run_as_user": "", "setup_env_default": false },
      "issue_base_prompt": "",
      "default_agent_kind": "llxprt",
      "agent_ids": ["agent-alpha", "agent-beta"]
    }
  ],
  "agents": [
    {
      "id": "agent-alpha",
      "display_id": "agent-alpha",
      "repository_id": "repo-230",
      "shortcut_slot": null,
      "name": "alpha",
      "description": "",
      "work_dir": "$WORKTREE_LLX",
      "profile": "ops",
      "code_puppy_model": "",
      "code_puppy_yolo": null,
      "code_puppy_quick_resume": false,
      "mode_flags": [],
      "llxprt_debug": "",
      "pass_continue": true,
      "sandbox_enabled": false,
      "sandbox_engine": "podman",
      "sandbox_flags": "",
      "agent_kind": "llxprt",
      "status": "Queued",
      "runtime_binding": null
    },
    {
      "id": "agent-beta",
      "display_id": "agent-beta",
      "repository_id": "repo-230",
      "shortcut_slot": null,
      "name": "beta",
      "description": "",
      "work_dir": "$WORKTREE_PUP",
      "profile": "",
      "code_puppy_model": "",
      "code_puppy_yolo": null,
      "code_puppy_quick_resume": false,
      "mode_flags": [],
      "llxprt_debug": "",
      "pass_continue": true,
      "sandbox_enabled": false,
      "sandbox_engine": "podman",
      "sandbox_flags": "",
      "agent_kind": "code_puppy",
      "status": "Queued",
      "runtime_binding": null
    }
  ],
  "selected_repository_index": 0,
  "selected_agent_index": null,
  "hide_idle_repositories": false,
  "last_selected_agent_by_repo": [],
  "pane_focus": "",
  "terminal_focused": false,
  "user_preferences": {}
}
EOF

# ── Build isolated PATH directory ────────────────────────────────────────
#
# The gh shim and the llxprt/code-puppy availability shims must be on the
# injected PATH so jefe detects both agent kinds as installed and routes all
# gh calls through the fail-closed shim.
rm -rf "$SHIM_DIR"
mkdir -p "$SHIM_DIR"

# Deploy the gh shim + its fixtures.
cp "$SHIM" "$SHIM_DIR/gh"
cp "$FIXTURES" "$SHIM_DIR/issue230-gh-shim-fixtures.sh"
chmod +x "$SHIM_DIR/gh"

# Deploy llxprt and code-puppy availability shims (issue #184 detection).
# These just need to exist and be executable so agent_detection marks both
# kinds as installed. They are never actually launched by this scenario.
cat > "$SHIM_DIR/llxprt" <<'EOF'
#!/usr/bin/env bash
# Availability shim: proves the LLxprt runtime executable is on PATH.
# This scenario never launches a real agent; the shim only needs to exist.
exit 0
EOF
cat > "$SHIM_DIR/code-puppy" <<'EOF'
#!/usr/bin/env bash
# Availability shim: proves the Code Puppy runtime executable is on PATH.
# This scenario never launches a real agent; the shim only needs to exist.
exit 0
EOF
chmod +x "$SHIM_DIR/llxprt" "$SHIM_DIR/code-puppy"

# Clean previous audit.
rm -f "$AUDIT_FILE"
export GH_SHIM_AUDIT="$AUDIT_FILE"

echo "Running scenario..."
cd "$PROJECT_ROOT"

harness_status=0
timeout 180s env \
    HOME="$ARTIFACT_DIR" \
    PATH="$SHIM_DIR:$PATH" \
    GH_SHIM_AUDIT="$AUDIT_FILE" \
    "$HARNESS_BIN" \
    --scenario "$SCENARIO" \
    --jefe-bin "$JEFE_BIN" \
    --config "$CONFIG_DIR" \
    --out-dir "$ARTIFACT_DIR" \
    --session "$SESSION_NAME" \
    "${HARNESS_ARGS[@]}" || harness_status=$?
if [[ $harness_status -eq 124 ]]; then
    echo "FAIL: harness timed out after 180 seconds" >&2
    cleanup_session
    echo ""
    echo "== Diagnostics: final screen =="
    cat "$ARTIFACT_DIR/final-screen.txt" 2>/dev/null || echo "(no final-screen.txt)"
    echo ""
    echo "== Diagnostics: error =="
    cat "$ARTIFACT_DIR/error.txt" 2>/dev/null || echo "(no error.txt)"
    exit 1
fi
if [[ $harness_status -ne 0 ]]; then
    cleanup_session
    echo ""
    echo "== Diagnostics: final screen =="
    cat "$ARTIFACT_DIR/final-screen.txt" 2>/dev/null || echo "(no final-screen.txt)"
    echo ""
    echo "== Diagnostics: error =="
    cat "$ARTIFACT_DIR/error.txt" 2>/dev/null || echo "(no error.txt)"
    exit "$harness_status"
fi
cleanup_session

echo ""
echo "== Verifying gh audit =="

if [[ ! -s "$AUDIT_FILE" ]]; then
    echo "FAIL: gh audit is missing or empty (shim was never invoked)."
    echo "  audit file: $AUDIT_FILE"
    exit 1
fi

echo "Audit log:"
cat "$AUDIT_FILE"

# ─── Reject any rejected or unexpected accepted operation ────────────────

if grep -qE '^\[[^]]+\] REJECTED ' -- "$AUDIT_FILE"; then
    echo ""
    echo "FAIL: rejected command detected in audit:"
    grep -E '^\[[^]]+\] REJECTED ' -- "$AUDIT_FILE"
    exit 1
fi

unexpected_operations=$(grep -oE 'ACCEPTED [A-Za-z0-9_-]+' -- "$AUDIT_FILE" \
    | sed 's/ACCEPTED //' \
    | grep -vE '^(search|issue-view|comments|auth-status)$' || true)
if [[ -n "$unexpected_operations" ]]; then
    echo ""
    echo "FAIL: unexpected accepted operation detected in audit:"
    echo "$unexpected_operations"
    exit 1
fi

# ─── Validate exact accepted command sequence ────────────────────────────
#
# The scenario must produce exactly these issue-read operations:
#   1. search         — initial issue list load
#   2. issue-view     — issue detail load
#   3. comments       — issue comments load
#
# The scenario opens the issue detail, opens the Send to Agent chooser, then
# escapes and quits — it does not navigate back to the issues list, so there
# is no refresh search (unlike the issue265 scenario).
#
# auth-status may appear if production invokes `gh auth status`; it is
# excluded from the three-operation sequence check.

accepted_seq=$(grep -oE 'ACCEPTED [A-Za-z0-9_-]+' -- "$AUDIT_FILE" \
    | sed 's/ACCEPTED //' \
    | grep -v '^auth-status$' || true)

if [[ -z "$accepted_seq" ]]; then
    echo "FAIL: no ACCEPTED issue-read operations in audit."
    exit 1
fi

accepted_count=$(echo "$accepted_seq" | wc -l | tr -d ' ')

echo ""
echo "Accepted issue-read sequence ($accepted_count ops):"
echo "$accepted_seq"

expected_seq="search
issue-view
comments"

if [[ "$accepted_seq" != "$expected_seq" ]]; then
    echo ""
    echo "FAIL: accepted issue-read sequence does not match expected four operations."
    echo "  expected:"
    echo "$expected_seq" | sed 's/^/    /'
    echo "  actual:"
    echo "$accepted_seq" | sed 's/^/    /'
    exit 1
fi

auth_count=$(grep -c 'ACCEPTED auth-status' -- "$AUDIT_FILE" || true)
if [[ "$auth_count" -gt 0 ]]; then
    echo "(auth-status calls: $auth_count — excluded from four-operation check)"
fi

echo ""
echo "PASS: exact accepted command sequence verified (search, issue-view, comments)."
echo "PASS: no mutations or rejected commands."
