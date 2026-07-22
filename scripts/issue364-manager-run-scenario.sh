#!/usr/bin/env bash
# Deterministic real-tmux runner for issue #364 terminal management.
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
SCENARIO="$PROJECT_ROOT/dev-docs/tmux-scenarios/terminal-manager.json"
if [[ ! -f "$SCENARIO" ]]; then
    echo "ERROR: issue #364 scenario not found: $SCENARIO" >&2
    exit 1
fi
HARNESS_ROOT="$PROJECT_ROOT/target/tmux-harness"
mkdir -p "$HARNESS_ROOT"
ARTIFACT_DIR="$(mktemp -d "$HARNESS_ROOT/issue364-manager-XXXXXX")"
CONFIG_DIR="$ARTIFACT_DIR/config"
WORK_DIR="$ARTIFACT_DIR/work"
BIN_DIR="$ARTIFACT_DIR/bin"
SESSION="jefe-issue364-manager-$(basename "$ARTIFACT_DIR")"
mkdir -p "$CONFIG_DIR" "$WORK_DIR/fixture-repo-alpha" "$WORK_DIR/fixture-repo-beta" "$BIN_DIR"

cat > "$BIN_DIR/llxprt" <<'EOF'
#!/bin/sh
printf 'issue364-agent-ready\n'
while IFS= read -r line; do
    printf '%s\n' "$line"
done
EOF
chmod +x "$BIN_DIR/llxprt"

cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml" --bin jefe --bin jefe-tmux-harness
JEFE_BIN="$PROJECT_ROOT/target/debug/jefe"
HARNESS_BIN="$PROJECT_ROOT/target/debug/jefe-tmux-harness"
for binary in "$JEFE_BIN" "$HARNESS_BIN"; do
    if [[ ! -x "$binary" ]]; then
        echo "ERROR: expected built executable not found: $binary" >&2
        exit 1
    fi
done
PATH="$BIN_DIR:$PATH" "$HARNESS_BIN" \
    --scenario "$SCENARIO" \
    --jefe-bin "$JEFE_BIN" \
    --config "$CONFIG_DIR" \
    --working-dir "$WORK_DIR" \
    --out-dir "$ARTIFACT_DIR" \
    --session "$SESSION"

echo "PASS: issue #364 terminal manager scenario"
echo "Artifacts: $ARTIFACT_DIR"
