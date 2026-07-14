#!/usr/bin/env bash
# Runner for the issue #230 real Linux tmux scenario.
#
# Sets up an isolated environment (temp HOME, config, state, PATH) seeded
# with a repository and two eligible local agents (one LLxprt with a
# configured profile, one Code Puppy with an empty model → default). Creates
# real local Git repos on known branches and makes one genuinely dirty
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
LLX_REPO_DIR="$ARTIFACT_DIR/wt-llxprt-alpha"
PUP_REPO_DIR="$ARTIFACT_DIR/wt-codepuppy-beta"
AUDIT_FILE="$ARTIFACT_DIR/gh-audit.log"

command -v timeout >/dev/null 2>&1 || {
    echo "FATAL: timeout is required for the Linux tmux scenario" >&2
    exit 1
}
# Verify GNU timeout behavior (exit 124 on timeout). BSD timeout (macOS) and
# some busybox implementations have different exit-code semantics or option
# syntax. The scenario and self-tests rely on exit 124 to detect hangs.
timeout_status=0
timeout 0.1s sleep 1 2>/dev/null || timeout_status=$?
if [[ $timeout_status -ne 124 ]]; then
    echo "FATAL: timeout does not behave like GNU coreutils timeout (exit code $timeout_status, expected 124)" >&2
    exit 1
fi
command -v realpath >/dev/null 2>&1 || {
    echo "FATAL: realpath is required for safe scenario cleanup" >&2
    exit 1
}
command -v python3 >/dev/null 2>&1 || {
    echo "FATAL: python3 is required for JSON state generation" >&2
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

# Tracks the overall exit status: 0 = success, non-zero = failure.
# On failure the artifact directory is preserved for debugging (unless
# --keep-session is also set, which preserves everything including tmux).
SCENARIO_STATUS=1

cleanup_session() {
    # Always kill the tmux session unless --keep-session was requested.
    if [[ "$KEEP_SESSION" == false ]]; then
        tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    fi
    # On success (SCENARIO_STATUS=0) and without --keep-session, also remove
    # the artifact directory so repeated runs don't accumulate. On failure,
    # preserve everything for debugging.
    if [[ "$SCENARIO_STATUS" -eq 0 && "$KEEP_SESSION" == false ]]; then
        rm -rf "$ARTIFACT_DIR" 2>/dev/null || true
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
require_target_descendant "$LLX_REPO_DIR"
require_target_descendant "$PUP_REPO_DIR"

rm -rf "$CONFIG_DIR" "$SHIM_DIR" "$REPO_DIR" "$LLX_REPO_DIR" "$PUP_REPO_DIR"
mkdir -p "$CONFIG_DIR" "$REPO_DIR"

# ── Seed settings.toml ──────────────────────────────────────────────────
cat > "$CONFIG_DIR/settings.toml" <<'EOF'
schema_version = 1
theme = "green-screen"
override_agent_theme = false
EOF

# ── Create real local Git repos on known branches ───────────────────────
#
# Each repo is an independent git repo (not a shared-object worktree) so
# that the branch name is deterministic and the dirty state is independently
# controllable.

# LLxprt agent repo: branch "main", will be made dirty.
mkdir -p "$LLX_REPO_DIR"
git -C "$LLX_REPO_DIR" init -q -b main
git -C "$LLX_REPO_DIR" config user.email "test@example.com"
git -C "$LLX_REPO_DIR" config user.name "Test User"
echo "# LLxprt repo" > "$LLX_REPO_DIR/README.md"
git -C "$LLX_REPO_DIR" add README.md
git -C "$LLX_REPO_DIR" commit -q -m "initial commit"

# Code Puppy agent repo: branch "feature", will stay clean.
mkdir -p "$PUP_REPO_DIR"
git -C "$PUP_REPO_DIR" init -q -b feature
git -C "$PUP_REPO_DIR" config user.email "test@example.com"
git -C "$PUP_REPO_DIR" config user.name "Test User"
echo "# Code Puppy repo" > "$PUP_REPO_DIR/README.md"
git -C "$PUP_REPO_DIR" add README.md
git -C "$PUP_REPO_DIR" commit -q -m "initial commit"

# Make the LLxprt repo genuinely dirty with a NON-owned change.
echo "uncommitted change" > "$LLX_REPO_DIR/src-change.txt"

# ── Seed state.json with repository + two eligible local agents ─────────
#
# Agent "alpha": LLxprt, profile "ops", repo on branch "main" (dirty).
# Agent "beta": Code Puppy, empty model (→ default), repo on branch
# "feature" (clean).
#
# State is generated with python3 and json.dump so filesystem paths are
# properly escaped (handles quotes, backslashes, control characters that may
# appear in temp paths on some systems).
python3 -c '
import json, sys

repo_dir = sys.argv[1]
llx_repo_dir = sys.argv[2]
pup_repo_dir = sys.argv[3]

state = {
    "schema_version": 1,
    "repositories": [
        {
            "id": "repo-230",
            "name": "repo-230",
            "slug": "repo-230",
            "base_dir": repo_dir,
            "default_profile": "",
            "default_code_puppy_model": "",
            "github_repo": "owner/repo-230",
            "github_issue_pr_repo": "",
            "remote": {
                "enabled": False,
                "login_user": "",
                "host": "",
                "run_as_user": "",
                "setup_env_default": False,
            },
            "issue_base_prompt": "",
            "default_agent_kind": "llxprt",
            "agent_ids": ["agent-alpha", "agent-beta"],
        }
    ],
    "agents": [
        {
            "id": "agent-alpha",
            "display_id": "agent-alpha",
            "repository_id": "repo-230",
            "shortcut_slot": None,
            "name": "alpha",
            "description": "",
            "work_dir": llx_repo_dir,
            "profile": "ops",
            "code_puppy_model": "",
            "code_puppy_yolo": None,
            "code_puppy_quick_resume": False,
            "mode_flags": [],
            "llxprt_debug": "",
            "pass_continue": True,
            "sandbox_enabled": False,
            "sandbox_engine": "podman",
            "sandbox_flags": "",
            "agent_kind": "llxprt",
            "status": "Queued",
            "runtime_binding": None,
        },
        {
            "id": "agent-beta",
            "display_id": "agent-beta",
            "repository_id": "repo-230",
            "shortcut_slot": None,
            "name": "beta",
            "description": "",
            "work_dir": pup_repo_dir,
            "profile": "",
            "code_puppy_model": "",
            "code_puppy_yolo": None,
            "code_puppy_quick_resume": False,
            "mode_flags": [],
            "llxprt_debug": "",
            "pass_continue": True,
            "sandbox_enabled": False,
            "sandbox_engine": "podman",
            "sandbox_flags": "",
            "agent_kind": "code_puppy",
            "status": "Queued",
            "runtime_binding": None,
        },
    ],
    "selected_repository_index": 0,
    "selected_agent_index": None,
    "hide_idle_repositories": False,
    "last_selected_agent_by_repo": [],
    "pane_focus": "",
    "terminal_focused": False,
    "user_preferences": {},
}

json.dump(state, sys.stdout, indent=2)
' "$REPO_DIR" "$LLX_REPO_DIR" "$PUP_REPO_DIR" > "$CONFIG_DIR/state.json"

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
    echo "FAIL: accepted issue-read sequence does not match expected three operations."
    echo "  expected:"
    echo "$expected_seq" | sed 's/^/    /'
    echo "  actual:"
    echo "$accepted_seq" | sed 's/^/    /'
    exit 1
fi

auth_count=$(grep -c 'ACCEPTED auth-status' -- "$AUDIT_FILE" || true)
if [[ "$auth_count" -gt 0 ]]; then
    echo "(auth-status calls: $auth_count — excluded from three-operation check)"
fi

echo ""
echo "PASS: exact accepted command sequence verified (search, issue-view, comments)."
echo "PASS: no mutations or rejected commands."

# All checks passed — mark success so the cleanup trap removes the artifact dir.
SCENARIO_STATUS=0
