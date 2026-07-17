#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
ARTIFACT="$ROOT/target/tmux-harness/issue351-$$"
CONFIG="$ARTIFACT/config"
SHIM_BIN="$ARTIFACT/bin"
REPO="$ARTIFACT/repo"
AUDIT="$ARTIFACT/gh-audit.log"
SESSION="jefe-issue351-$$"

run_with_timeout() {
  local timeout_seconds="$1"
  shift
  python3 - "$timeout_seconds" "$@" <<'PY'
import subprocess
import sys

seconds = int(sys.argv[1])
command = sys.argv[2:]
try:
    result = subprocess.run(command, timeout=seconds, check=False)
except subprocess.TimeoutExpired:
    print(f"FATAL: command timed out after {seconds} seconds: {' '.join(command)}", file=sys.stderr)
    raise SystemExit(124)
raise SystemExit(result.returncode)
PY
}
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
def repository(identifier, name, github_repo):
    return {
        "id": identifier,
        "name": name,
        "slug": name,
        "base_dir": repo,
        "default_profile": "",
        "default_code_puppy_model": "",
        "github_repo": github_repo,
        "remote": {"enabled": False, "host": "", "user": "", "port": None},
        "issue_base_prompt": "",
        "default_agent_kind": "llxprt",
        "agent_ids": [],
    }

state = {
    "schema_version": 1,
    "repositories": [
        repository("with-issues", "with-issues", "owner/with-issues"),
        repository("empty-issues", "empty-issues", "owner/empty-issues"),
    ],
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
SHIM_SOURCE="$ROOT/scripts/issue351-gh-shim.sh"
[[ -s "$SHIM_SOURCE" ]] || { echo "FATAL: missing or empty gh shim: $SHIM_SOURCE" >&2; exit 1; }
cp "$SHIM_SOURCE" "$SHIM_BIN/gh"
chmod +x "$SHIM_BIN/gh"

(cd "$ROOT" && run_with_timeout 300 cargo build --locked --bin jefe --bin jefe-tmux-harness)
run_with_timeout 300 env PATH="$SHIM_BIN:$PATH" GH_SHIM_AUDIT="$AUDIT" RUST_BACKTRACE=1 \
  "$ROOT/target/debug/jefe-tmux-harness" \
  --scenario "$ROOT/dev-docs/tmux-scenarios/issues-empty-repository.json" \
  --jefe-bin "$ROOT/target/debug/jefe" \
  --config "$CONFIG" --out-dir "$ARTIFACT" --session "$SESSION"

[[ -s "$AUDIT" ]] || { echo "FATAL: gh shim was not invoked" >&2; exit 1; }
if grep -q REJECTED "$AUDIT"; then cat "$AUDIT" >&2; exit 1; fi
for expected in "searchQuery=repo:owner/with-issues" "searchQuery=repo:owner/empty-issues"; do
  grep -F -- "$expected" "$AUDIT" >/dev/null || { echo "FATAL: missing audit operation: $expected" >&2; cat "$AUDIT" >&2; exit 1; }
done
printf 'PASS: issue 351 empty-repository navigation remains stable and panic-free\n'
