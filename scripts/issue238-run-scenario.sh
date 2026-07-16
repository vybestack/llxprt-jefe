#!/usr/bin/env bash
# Runner for issue #238 TUI scenario: pr-review-newest-first.
#
# Seeds an isolated config with a repository whose github_repo points at
# the gh-shim fixture, builds jefe + the tmux harness, runs the scenario,
# and verifies the gh shim was invoked with only the expected read-only
# operations.
#
# Requirements: cargo, python3, tmux.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
ARTIFACT="$ROOT/target/tmux-harness/issue238-$$"
CONFIG="$ARTIFACT/config"
SHIM_BIN="$ARTIFACT/bin"
REPO="$ARTIFACT/repo"
AUDIT="$ARTIFACT/gh-audit.log"
SESSION="jefe-issue238-$$"

cleanup() {
  if tmux has-session -t "$SESSION" 2>/dev/null; then
    tmux kill-session -t "$SESSION" || echo "WARN: failed to stop tmux session $SESSION" >&2
  fi
  rm -rf "$ARTIFACT"
}
trap cleanup EXIT

for command_name in cargo python3 tmux; do
  command -v "$command_name" >/dev/null || { echo "FATAL: $command_name is required" >&2; exit 1; }
done

mkdir -p "$CONFIG" "$SHIM_BIN" "$REPO"
cat > "$CONFIG/settings.toml" <<'EOF'
schema_version = 1
theme = "green-screen"
override_agent_theme = false
EOF
python3 - "$REPO" "$CONFIG/state.json" <<'PY'
import json
import sys

repo, output = sys.argv[1:]
state = {
    "schema_version": 1,
    "repositories": [{
        "id": "issue238-repo",
        "name": "review-sort-fixture",
        "slug": "review-sort-fixture",
        "base_dir": repo,
        "default_profile": "",
        "default_code_puppy_model": "",
        "github_repo": "owner/review-sort-fixture",
        "remote": {"enabled": False, "host": "", "user": "", "port": None},
        "issue_base_prompt": "",
        "default_agent_kind": "llxprt",
        "agent_ids": [],
    }],
    "agents": [],
    "selected_repository_index": 0,
    "selected_agent_index": None,
    "hide_idle_repositories": False,
    "last_selected_agent_by_repo": [],
    "pane_focus": "",
    "terminal_focused": False,
    "user_preferences": {},
}
with open(output, "w", encoding="utf-8") as stream:
    json.dump(state, stream)
PY
SHIM_SOURCE="$ROOT/scripts/issue238-gh-shim.sh"
SCENARIO_JSON="$ROOT/dev-docs/tmux-scenarios/pr-review-newest-first.json"
[[ -s "$SHIM_SOURCE" ]] || { echo "FATAL: missing or empty gh shim: $SHIM_SOURCE" >&2; exit 1; }
[[ -s "$SCENARIO_JSON" ]] || { echo "FATAL: missing or empty scenario JSON: $SCENARIO_JSON" >&2; exit 1; }
cp "$SHIM_SOURCE" "$SHIM_BIN/gh"
chmod +x "$SHIM_BIN/gh"

(cd "$ROOT" && cargo build --locked --bin jefe --bin jefe-tmux-harness)

JEFE_BIN="$ROOT/target/debug/jefe"
HARNESS_BIN="$ROOT/target/debug/jefe-tmux-harness"
[[ -x "$JEFE_BIN" ]] || { echo "FATAL: build did not produce $JEFE_BIN" >&2; exit 1; }
[[ -x "$HARNESS_BIN" ]] || { echo "FATAL: build did not produce $HARNESS_BIN" >&2; exit 1; }

env PATH="$SHIM_BIN:$PATH" GH_SHIM_AUDIT="$AUDIT" \
  "$HARNESS_BIN" \
  --scenario "$SCENARIO_JSON" \
  --jefe-bin "$JEFE_BIN" \
  --config "$CONFIG" --out-dir "$ARTIFACT" --session "$SESSION"

[[ -s "$AUDIT" ]] || { echo "FATAL: gh shim was not invoked" >&2; exit 1; }
if grep -q REJECTED "$AUDIT"; then cat "$AUDIT" >&2; exit 1; fi
# Fail if the shim recorded any write-mutation operations. The scenario
# only loads PR data read-only; any write op indicates a logic error or
# shim bypass.
if grep -qE 'ACCEPTED (pr-merge|pr-close|pr-update|comment-create|pr-review|pr-ready|issue-create|issue-update|issue-close)' "$AUDIT"; then
  echo "FATAL: unexpected write operation in gh audit:" >&2
  cat "$AUDIT" >&2
  exit 1
fi
for expected in "pr-search" "pr-view-detail" "review-threads"; do
  grep -F -- "$expected" "$AUDIT" >/dev/null || { echo "FATAL: missing audit operation: $expected" >&2; cat "$AUDIT" >&2; exit 1; }
done
printf 'PASS: issue 238 PR review newest-first scenario and read-only gh audit\n'
