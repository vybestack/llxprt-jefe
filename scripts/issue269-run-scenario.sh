#!/usr/bin/env bash
# Runner for the issue #269 LLxprt version fields tmux scenario.
#
# Sets up an isolated config directory with the current Jefe persistence
# schema, runs the scenario via the real tmux driver, and verifies that
# the LLxprt Version / Default Version fields appear for LLxprt agents
# and repositories, are editable, and are hidden when the agent kind is
# Code Puppy.
#
# Usage:
#   scripts/issue269-run-scenario.sh [--keep-session]
#
# Requirements: tmux 3.x, cargo, the jefe + harness binaries (built if missing).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
SCENARIO="$PROJECT_ROOT/dev-docs/tmux-scenarios/llxprt-version-fields.json"
HARNESS_PARENT="$PROJECT_ROOT/target/tmux-harness"
mkdir -p "$HARNESS_PARENT"
ARTIFACT_DIR="$(mktemp -d "$HARNESS_PARENT/issue269-XXXXXX")"
CONFIG_DIR="$ARTIFACT_DIR/config"
REPO_DIR="$ARTIFACT_DIR/repo"

command -v cargo >/dev/null 2>&1 || {
    echo "FATAL: cargo is required for the issue #269 scenario" >&2
    exit 1
}
command -v tmux >/dev/null 2>&1 || {
    echo "FATAL: tmux is required for the issue #269 scenario" >&2
    exit 1
}

[[ -r "$SCENARIO" ]] || {
    echo "FATAL: scenario file is missing or not readable: $SCENARIO" >&2
    exit 1
}

# Optional extra args passed to the harness binary (array, properly quoted).
HARNESS_ARGS=()
KEEP_SESSION=false
if [[ "${1:-}" == "--keep-session" ]]; then
    HARNESS_ARGS+=("--keep-session")
    KEEP_SESSION=true
fi
SESSION_NAME="jefe-issue269-$(basename "$ARTIFACT_DIR")"

# Tracks the overall exit status: 0 = success, non-zero = failure.
# On failure the artifact directory is preserved for debugging (unless
# --keep-session is also set, which preserves everything including tmux).
SCENARIO_STATUS=1

_CLEANUP_DONE=false

cleanup_session() {
    [[ "$_CLEANUP_DONE" == true ]] && return 0
    _CLEANUP_DONE=true
    if [[ "$KEEP_SESSION" == false ]]; then
        tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    fi
    if [[ "$SCENARIO_STATUS" -eq 0 && "$KEEP_SESSION" == false ]]; then
        rm -rf "$ARTIFACT_DIR" 2>/dev/null || true
    fi
}
trap cleanup_session EXIT

echo "== Issue #269 LLxprt version fields scenario =="

echo "Building jefe and jefe-tmux-harness (incremental)..."
build_status=0
cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml" --bin jefe --bin jefe-tmux-harness 2>&1 || build_status=$?
if [[ $build_status -ne 0 ]]; then
    echo "FATAL: cargo build failed (exit $build_status)" >&2
    exit 1
fi

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

rm -rf "$CONFIG_DIR"
mkdir -p "$CONFIG_DIR" "$REPO_DIR"

cat > "$CONFIG_DIR/settings.toml" <<'EOF'
schema_version = 1
theme = "green-screen"
override_agent_theme = false
EOF

cat > "$CONFIG_DIR/state.json" <<EOF
{
  "schema_version": 1,
  "repositories": [
    {
      "id": "repo-269",
      "name": "repo-269",
      "slug": "repo-269",
      "base_dir": "$REPO_DIR",
      "default_profile": "",
      "default_code_puppy_model": "",
      "github_repo": "",
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

echo "Running scenario..."
cd "$PROJECT_ROOT"

harness_status=0
"$HARNESS_BIN" \
    --scenario "$SCENARIO" \
    --jefe-bin "$JEFE_BIN" \
    --config "$CONFIG_DIR" \
    --out-dir "$ARTIFACT_DIR" \
    --session "$SESSION_NAME" \
    ${HARNESS_ARGS[@]+"${HARNESS_ARGS[@]}"} || harness_status=$?

if [[ $harness_status -ne 0 ]]; then
    echo ""
    echo "== Diagnostics: final screen =="
    cat "$ARTIFACT_DIR/final-screen.txt" 2>/dev/null || echo "(no final-screen.txt)"
    echo ""
    echo "== Diagnostics: error =="
    cat "$ARTIFACT_DIR/error.txt" 2>/dev/null || echo "(no error.txt)"
    exit "$harness_status"
fi

echo ""
echo "PASS: LLxprt version fields scenario completed successfully."
echo "  Artifacts: $ARTIFACT_DIR"

SCENARIO_STATUS=0
