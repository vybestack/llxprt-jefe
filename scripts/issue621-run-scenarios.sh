#!/usr/bin/env bash
# Runs the two issue #621 direct list-send TUI scenarios with read-only fixtures.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
ARTIFACT_ROOT="$ROOT/target/tmux-harness"
mkdir -p "$ARTIFACT_ROOT"
ARTIFACT="$(mktemp -d "$ARTIFACT_ROOT/issue621.XXXXXX")"
CONFIG="$ARTIFACT/config"
SHIM_BIN="$ARTIFACT/bin"
REPO="$ARTIFACT/repo"
WORKTREE="$ARTIFACT/worktree"
SCENARIO_WORKSPACE="$ARTIFACT/scenario-workspace"
RUN_SUCCEEDED=0
SESSIONS=()

cleanup() {
    status=$?
    trap - EXIT
    for session in ${SESSIONS[@]+"${SESSIONS[@]}"}; do
        if tmux has-session -t "$session" 2>/dev/null; then
            tmux kill-session -t "$session" || echo "WARN: failed to stop tmux session $session" >&2
        fi
    done
    if (( RUN_SUCCEEDED )); then
        rm -rf "$ARTIFACT"
    else
        echo "Diagnostics retained at $ARTIFACT" >&2
    fi
    exit "$status"
}
trap cleanup EXIT
trap cleanup INT TERM HUP

for command_name in awk cargo git grep python3 tmux; do
    command -v "$command_name" >/dev/null || {
        echo "FATAL: $command_name is required" >&2
        exit 1
    }
done

mkdir -p "$CONFIG" "$SHIM_BIN" "$REPO" "$WORKTREE" "$SCENARIO_WORKSPACE"
cat > "$CONFIG/settings.toml" <<'EOF'
settings_schema = 2
EOF

git -C "$WORKTREE" init -q
git -C "$WORKTREE" config user.email "fixture@example.com"
git -C "$WORKTREE" config user.name "Fixture User"
printf '# issue 621 fixture\n' > "$WORKTREE/README.md"
git -C "$WORKTREE" add README.md
git -C "$WORKTREE" commit -q -m "fixture"

python3 - "$REPO" "$WORKTREE" "$CONFIG/state.json" <<'PY'
import json
import sys

repo, worktree, output = sys.argv[1:]
state = {
    "schema_version": 1,
    "repositories": [{
        "id": "issue621-repo",
        "name": "list-send-fixture",
        "slug": "list-send-fixture",
        "base_dir": repo,
        "default_profile": "",
        "default_code_puppy_model": "",
        "github_repo": "owner/list-send-fixture",
        "github_issue_pr_repo": "",
        "remote": {
            "enabled": True,
            "login_user": "fixture-user",
            "host": "fixture-host",
            "run_as_user": "",
            "setup_env_default": False,
        },
        "issue_base_prompt": "",
        "default_agent_kind": "llxprt",
        "agent_ids": ["issue621-agent"],
    }],
    "agents": [{
        "id": "issue621-agent",
        "display_id": "issue621-agent",
        "repository_id": "issue621-repo",
        "shortcut_slot": None,
        "name": "fixture-agent",
        "description": "",
        "work_dir": worktree,
        "profile": "list-send",
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
    }],
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
cp "$CONFIG/state.json" "$ARTIFACT/base-state.json"

SHIM_SOURCE="$ROOT/scripts/issue621-gh-shim.sh"
[[ -s "$SHIM_SOURCE" ]] || {
    echo "FATAL: missing gh shim: $SHIM_SOURCE" >&2
    exit 1
}
cp "$SHIM_SOURCE" "$SHIM_BIN/gh"
chmod +x "$SHIM_BIN/gh"
cat > "$SHIM_BIN/llxprt" <<'EOF'
#!/usr/bin/env sh
if [ "${1:-}" = "--version" ]; then
    printf '0.10.0\n'
fi
exit 0
EOF
chmod +x "$SHIM_BIN/llxprt"

(
    cd "$ROOT"
    cargo build --locked --bin jefe --bin jefe-tmux-harness
)
JEFE_BIN="$ROOT/target/debug/jefe"
HARNESS_BIN="$ROOT/target/debug/jefe-tmux-harness"
[[ -x "$JEFE_BIN" && -x "$HARNESS_BIN" ]] || {
    echo "FATAL: scenario binaries were not built" >&2
    exit 1
}

run_scenario() {
    local kind="$1"
    local scenario="$ROOT/dev-docs/tmux-scenarios/issue621/${kind}-list-send-agent.json"
    local audit="$ARTIFACT/${kind}-gh-audit.log"
    local out_dir="$ARTIFACT/$kind"
    local session="jefe-issue621-${kind}-${ARTIFACT##*.}"
    SESSIONS+=("$session")
    cp "$ARTIFACT/base-state.json" "$CONFIG/state.json"
    mkdir -p "$out_dir"
    [[ -s "$scenario" ]] || {
        echo "FATAL: missing scenario: $scenario" >&2
        exit 1
    }

    env HOME="$ARTIFACT" PATH="$SHIM_BIN:$PATH" JEFE_GH_AUDIT="$audit" \
        "$HARNESS_BIN" \
        --scenario "$scenario" \
        --jefe-bin "$JEFE_BIN" \
        --config "$CONFIG" \
        --out-dir "$out_dir" \
        --session "$session" \
        --working-dir "$SCENARIO_WORKSPACE"

    [[ -s "$audit" ]] || {
        echo "FATAL: gh shim was not invoked for $kind" >&2
        exit 1
    }
    if grep -q '^REJECTED ' "$audit"; then
        cat "$audit" >&2
        exit 1
    fi
    local actual
    actual="$(awk '$1 == "ACCEPTED" { print $2 }' "$audit")"
    if [[ "$kind" == "issues" ]]; then
        local expected_issue_sequence=$'issue-search\nissue-view-detail\nissue-comments'
        if [[ "$actual" != "$expected_issue_sequence" ]]; then
            echo "FATAL: unexpected complete operation sequence for $kind" >&2
            printf 'expected sequence:\n%s\nactual sequence:\n%s\n' \
                "$expected_issue_sequence" "$actual" >&2
            cat "$audit" >&2
            exit 1
        fi
    else
        local pr_detail_first=$'pr-search\npr-view-detail\npr-comments\npr-review-threads'
        local pr_detail_threads_first=$'pr-search\npr-view-detail\npr-review-threads\npr-comments'
        local pr_comments_first=$'pr-search\npr-comments\npr-view-detail\npr-review-threads'
        local pr_comments_threads_first=$'pr-search\npr-comments\npr-review-threads\npr-view-detail'
        local pr_threads_first=$'pr-search\npr-review-threads\npr-view-detail\npr-comments'
        local pr_threads_comments_first=$'pr-search\npr-review-threads\npr-comments\npr-view-detail'
        case "$actual" in
            "$pr_detail_first" | "$pr_detail_threads_first" | "$pr_comments_first" | \
                "$pr_comments_threads_first" | "$pr_threads_first" | \
                "$pr_threads_comments_first") ;;
            *)
                echo "FATAL: unexpected complete operation sequence for $kind" >&2
                printf 'actual sequence:\n%s\n' "$actual" >&2
                cat "$audit" >&2
                exit 1
                ;;
        esac
    fi
}

run_scenario issues
run_scenario prs
RUN_SUCCEEDED=1
printf 'PASS: issue 621 issue-list and PR-list send-to-agent scenarios\n'
