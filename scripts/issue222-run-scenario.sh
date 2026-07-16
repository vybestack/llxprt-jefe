#!/usr/bin/env bash
# Deterministic real-tmux runner for issue #222.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
SCENARIO="$PROJECT_ROOT/dev-docs/tmux-scenarios/agent-shell-overlay.json"
HARNESS_ROOT="$PROJECT_ROOT/target/tmux-harness"
mkdir -p "$HARNESS_ROOT"
ARTIFACT_DIR="$(mktemp -d "$HARNESS_ROOT/issue222-XXXXXX")"
CONFIG_DIR="$ARTIFACT_DIR/config"
WORK_DIR="$ARTIFACT_DIR/work"
BIN_DIR="$ARTIFACT_DIR/bin"
SESSION="jefe-issue222-$(basename "$ARTIFACT_DIR")"
mkdir -p "$CONFIG_DIR" "$WORK_DIR/fixture-repo" "$BIN_DIR"

cat > "$BIN_DIR/llxprt" <<'EOF'
#!/bin/sh
printf 'issue222-agent-ready\n'
while IFS= read -r line; do
    printf '%s\n' "$line"
done
EOF
chmod +x "$BIN_DIR/llxprt"

cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml" --bin jefe --bin jefe-tmux-harness
PATH="$BIN_DIR:$PATH" "$PROJECT_ROOT/target/debug/jefe-tmux-harness" \
    --scenario "$SCENARIO" \
    --jefe-bin "$PROJECT_ROOT/target/debug/jefe" \
    --config "$CONFIG_DIR" \
    --working-dir "$WORK_DIR" \
    --out-dir "$ARTIFACT_DIR" \
    --session "$SESSION"

echo "PASS: issue #222 agent shell scenario"
echo "Artifacts: $ARTIFACT_DIR"
