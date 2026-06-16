#!/usr/bin/env bash
set -euo pipefail

errors=0

fail() {
  echo "ERROR: $*" >&2
  errors=$((errors + 1))
}

# Keep migration pressure on the message architecture: new crate-wide clippy
# allow attributes hide domain boundary regressions and must be reviewed.
while IFS= read -r file; do
  fail "global clippy allow attribute is not permitted: $file"
done < <(
  find src tests -type f -name '*.rs' -print0 \
    | xargs -0 grep -nE '^#!\[(cfg_attr\([^]]*clippy|allow\([^]]*clippy)' \
    | grep -Ev '(src/main.rs:6:#!\[allow\(clippy::print_stderr\)\]|src/main.rs:7:#!\[allow\(clippy::collapsible_if\)\]|src/main.rs:8:#!\[allow\(clippy::clone_on_copy\)\]|src/main.rs:9:#!\[allow\(clippy::significant_drop_tightening\)\]|src/runtime/mod.rs:29:#!\[allow\(clippy::expect_used\)\]|tests/e2e/end_to_end.rs:13:#!\[allow\(clippy::unwrap_used, clippy::expect_used\)\]|tests/e2e/recovery_paths.rs:11:#!\[allow\(clippy::unwrap_used, clippy::expect_used\)\]|tests/core/persistence_theme_contracts.rs:10:#!\[allow\(clippy::expect_used\)\]|tests/core/domain_state_contracts.rs:9:#!\[allow\(clippy::expect_used\)\]|tests/core/domain_state_contracts.rs:10:#!\[allow\(clippy::unwrap_used\)\]|tests/runtime/terminal_focus_routing.rs:9:#!\[allow\(clippy::expect_used\)\]|tests/runtime/terminal_focus_routing.rs:10:#!\[allow\(clippy::unwrap_used\)\]|tests/core/visibility_filter_contracts.rs:3:#!\[allow\(clippy::expect_used\)\]|tests/core/visibility_filter_contracts.rs:4:#!\[allow\(clippy::unwrap_used\)\]|tests/runtime/runtime_lifecycle.rs:9:#!\[allow\(clippy::expect_used\)\]|tests/runtime/runtime_lifecycle.rs:10:#!\[allow\(clippy::unwrap_used\)\])' \
    || true
)

if ! grep -R "pub enum AppMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the typed AppMessage bus"
fi

if ! grep -R "pub enum UiNavigationMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the ui_navigation channel"
fi

if ! grep -R "pub enum ModalMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the modal channel"
fi

if ! grep -R "pub enum RepositoryAgentMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the repository_agent channel"
fi

if ! grep -R "pub enum RuntimeMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the runtime channel"
fi

if ! grep -R "pub enum PersistenceMessage" -n src/messages.rs >/dev/null; then
  fail "src/messages.rs must define the persistence channel"
fi

if ! grep -R "pub fn apply_message" -n src/state/mod.rs >/dev/null; then
  fail "AppState must expose apply_message for routed state transitions"
fi

if ! grep -R "pub fn dispatch_app_message" -n src/app_input/mod.rs >/dev/null; then
  fail "app input must dispatch routed AppMessage values"
fi

handler_limit=850
while IFS= read -r file; do
  lines=$(wc -l < "$file")
  if [ "$lines" -gt "$handler_limit" ]; then
    fail "new handler module $file has $lines lines (max $handler_limit)"
  fi
done < <(git ls-files 'src/app_input/*ops.rs' 'src/app_input/*handlers.rs' 'src/app_input/*dispatch.rs' 'src/state/*ops.rs' 'src/state/*handlers.rs' 'src/state/*dispatch.rs' --others --exclude-standard)

if [ "$errors" -gt 0 ]; then
  echo "Architecture boundary checks failed with $errors error(s)." >&2
  exit 1
fi

echo "Architecture boundary checks passed."
