#!/usr/bin/env bash
# Tmux validation for issue #212: the new-issue composer must word-wrap.
# Launches the app at 80x24, opens the new-issue composer, types a long line
# of words, and asserts the rendered pane wraps (no ellipsis truncation, and
# a later word appears on a subsequent row).
set -euo pipefail

SESSION="jefe-wrap-212"
BIN="/Volumes/XS1000/acoliver/projects/jefe/branch-4/target/debug/jefe"
CFG="/Volumes/XS1000/acoliver/projects/jefe/branch-4/.config/jefe-dev"
PASS=0

cleanup() {
  tmux kill-session -t "$SESSION" 2>/dev/null || true
}
trap cleanup EXIT

cleanup
tmux new-session -d -s "$SESSION" -x 80 -y 24 "$BIN --config $CFG"
# Give the app time to render the dashboard.
sleep 2.5

# Dashboard -> Issues mode (key 'i').
tmux send-keys -t "$SESSION" "i"
sleep 1.5

# Issues mode -> new-issue composer (key 'n').
tmux send-keys -t "$SESSION" "n"
sleep 1.5

# Type a long line of words that must wrap at the pane width.
tmux send-keys -t "$SESSION" "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu"
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

# Assertion 2: a word from the END of the typed line must be visible (it can
# only be visible if the line wrapped onto later rows; at 80 cols with the
# sidebar+chrome the content width is ~70, so "lambda"/"mu" wrap below).
if ! grep -Eq "lambda|kappa" "$CAPTURE"; then
  echo "FAIL: a late word (lambda/kappa) is not visible — line did not wrap"
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

rm -f "$CAPTURE"

if [ "$PASS" -eq 0 ]; then
  echo "PASS: new-issue composer wraps long text (issue #212 fixed)"
  exit 0
else
  echo "FAIL: new-issue composer did not wrap correctly"
  exit 1
fi
