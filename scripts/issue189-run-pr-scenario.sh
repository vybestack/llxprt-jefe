#!/usr/bin/env bash
# Runner for issue #189 TUI scenario: pr-composer-submit-key.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
ARTIFACT_ROOT="$ROOT/target/tmux-harness"
mkdir -p "$ARTIFACT_ROOT"
ARTIFACT="$(mktemp -d "$ARTIFACT_ROOT/issue189-pr.XXXXXX")"
CONFIG="$ARTIFACT/config"
SHIM_BIN="$ARTIFACT/bin"
REPO="$ARTIFACT/repo"
AUDIT="$ARTIFACT/gh-audit.log"
SESSION="jefe-issue189-pr-${ARTIFACT##*.}"
RUN_SUCCEEDED=0

cleanup() {
  status=$?
  trap - EXIT
  if tmux has-session -t "$SESSION" 2>/dev/null; then
    tmux kill-session -t "$SESSION" || echo "WARN: failed to stop tmux session $SESSION" >&2
  fi
  if (( RUN_SUCCEEDED )); then
    rm -rf "$ARTIFACT"
  else
    echo "Diagnostics retained at $ARTIFACT" >&2
  fi
  exit "$status"
}
trap cleanup EXIT

for command_name in cargo python3 tmux; do
  command -v "$command_name" >/dev/null || { echo "FATAL: $command_name is required" >&2; exit 1; }
done

mkdir -p "$CONFIG" "$SHIM_BIN" "$REPO"
cat > "$CONFIG/settings.toml" <<'EOF'
settings_schema = 2
[keymap."prs.inline"]
"prs.inline-submit" = ["F9"]
EOF
python3 - "$REPO" "$CONFIG/state.json" <<'PY'
import json
import sys

repo, output = sys.argv[1:]
state = {
    "schema_version": 1,
    "repositories": [{
        "id": "issue189-repo",
        "name": "composer-submit-fixture",
        "slug": "composer-submit-fixture",
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
SHIM_SOURCE="$ROOT/scripts/issue189-gh-shim.sh"
SCENARIO_JSON="$ROOT/dev-docs/tmux-scenarios/pr-composer-submit-key.json"
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
if grep -q '^REJECTED ' "$AUDIT"; then cat "$AUDIT" >&2; exit 1; fi
if grep '^ACCEPTED ' "$AUDIT" | grep -qvE '^ACCEPTED (auth-status|pr-search|review-threads|pr-comments|pr-view-detail) -- gh '; then
  echo "FATAL: unexpected accepted operation in gh audit:" >&2
  cat "$AUDIT" >&2
  exit 1
fi
for expected in "pr-search" "pr-view-detail" "review-threads"; do
  grep -F -- "$expected" "$AUDIT" >/dev/null || { echo "FATAL: missing audit operation: $expected" >&2; cat "$AUDIT" >&2; exit 1; }
done
RUN_SUCCEEDED=1
printf 'PASS: issue 189 PR composer submit-key scenario and read-only gh audit\n'
