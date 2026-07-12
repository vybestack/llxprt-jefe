#!/usr/bin/env bash
# Tmux validation for issue #212: the new-issue composer must word-wrap.
#
# Launches the app at 80x24, opens the new-issue composer, types a long line
# of words, and asserts the rendered pane wraps (no ellipsis truncation, and
# a later word appears on a subsequent row).
#
# Portable: resolves the repo root from this script's location and builds the
# debug binary itself. Requires `cargo`, `tmux`, and a writable Cargo target
# dir. Skips (exits 0) when tmux is unavailable (CI without tmux).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CFG="$REPO_ROOT/.config/jefe-dev"
SESSION="jefe-wrap-212-$$" # include PID for uniqueness across concurrent runs
CAPTURE=""

if ! command -v tmux >/dev/null 2>&1; then
  echo "SKIP: tmux not installed"
  exit 0
fi

cleanup() {
  if [ -n "${TMUX_PID:-}" ]; then
    tmux kill-session -t "$SESSION" 2>/dev/null || true
  fi
  [ -n "$CAPTURE" ] && rm -f "$CAPTURE"
}
trap cleanup EXIT

# Build the debug binary (errors out via set -e if the build fails).
echo "Building debug binary..."
cargo build --quiet --bin jefe
BIN="$REPO_ROOT/target/debug/jefe"
if [ ! -x "$BIN" ]; then
  echo "FAIL: binary not found at $BIN after build"
  exit 1
fi

PASS=0
tmux new-session -d -s "$SESSION" -x 80 -y 24 "$BIN --config $CFG"
TMUX_PID=$$
# Give the app time to render the dashboard.
sleep 2.5

# Dashboard -> Issues mode (key 'i').
tmux send-keys -t "$SESSION" "i"
sleep 1.5

# Issues mode -> new-issue composer (key 'n').
tmux send-keys -t "$SESSION" "n"
sleep 1.5

# Type a long line of words that must wrap at the pane width.
tmux send-keys -t "$SESSION" \
  "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron"
sleep 1.5

CAPTURE="$(mktemp)"
tmux capture-pane -p -t "$SESSION" > "$CAPTURE"

echo "===== CAPTURED PANE (new-issue composer) ====="
cat "$CAPTURE"
echo "===== END CAPTURE ====="

# Assertion 1: no ellipsis truncation.
if grep -q $'\u2026' "$CAPTURE"; then
  echo "FAIL: pane contains an ellipsis (truncation, not wrapping)"
  PASS=1
fi

# Assertion 2: the first and last typed words must appear on DIFFERENT rows,
# proving the line wrapped (the input is far longer than the pane width).
first_line=$(grep -nE "alpha" "$CAPTURE" | head -1 | cut -d: -f1)
last_line=$(grep -nE "omicron|lambda|nu" "$CAPTURE" | tail -1 | cut -d: -f1)
if [ -z "$first_line" ] || [ -z "$last_line" ]; then
  echo "FAIL: could not find first (alpha) or last (omicron/lambda/nu) word"
  PASS=1
elif [ "$first_line" -ge "$last_line" ]; then
  echo "FAIL: first word (line $first_line) not above last word (line $last_line) — no wrap"
  PASS=1
fi

# Assertion 3: no line exceeds the 80-col terminal width. Use `wc -m` per line
# (character count) because box-drawing chars are multibyte and byte-length
# (`awk length`) overcounts.
LONGEST=0
while IFS= read -r line; do
  n=$(printf '%s' "$line" | wc -m | tr -d ' ')
  if [ "$n" -gt "$LONGEST" ]; then LONGEST=$n; fi
done < "$CAPTURE"
if [ "$LONGEST" -gt 80 ]; then
  echo "FAIL: a rendered line exceeds 80 columns (max=$LONGEST)"
  PASS=1
fi

if [ "$PASS" -eq 0 ]; then
  echo "PASS: new-issue composer wraps long text (issue #212 fixed)"
  exit 0
else
  echo "FAIL: new-issue composer did not wrap correctly"
  exit 1
fi
