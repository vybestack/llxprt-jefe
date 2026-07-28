#!/usr/bin/env bash
set -euo pipefail

cargo xtask check architecture

fail_if_matches() {
  local message="$1"
  local pattern="$2"
  shift 2
  if grep -R -n -E "$pattern" "$@"; then
    printf '%s\n' "$message" >&2
    exit 1
  fi
}

fail_before_test_modules() {
  local message="$1"
  local pattern="$2"
  local found=0
  while IFS= read -r file; do
    if sed '/^[[:space:]]*#\[cfg(test)\]/,$d' "$file" | grep -v -E '^[[:space:]]*//' | grep -n -E "$pattern" | sed "s#^#$file:#"; then
      found=1
    fi
  done < <(find src -type f -name '*.rs' \
    ! -name '*_tests.rs' ! -name '*tests.rs' \
    ! -name '*_test_*.rs' ! -name '*tests_*.rs' | sort)
  if [[ "$found" -ne 0 ]]; then
    printf '%s\n' "$message" >&2
    exit 1
  fi
}

fail_if_matches \
  'issue #382 architecture check: legacy agent authority remains outside migration' \
  'AgentKind|AgentLaunchConfiguration|default_agent_kind|installed_agent_kinds' \
  src --include='*.rs' --exclude='migration*.rs'

fail_if_matches \
  'issue #382 architecture check: runtime branches on product identity' \
  'core\.(llxprt|code-puppy|claude|codex)|code-puppy|AgentKind' \
  src/runtime --include='*.rs' --exclude='*_tests.rs' --exclude='*tests.rs'

fail_if_matches \
  'issue #382 architecture check: active persistence branches on product identity' \
  'core\.(llxprt|code-puppy|claude|codex)|code-puppy|AgentKind|agent_kind' \
  src/persistence --include='*.rs' --exclude='migration*.rs' --exclude='*_tests.rs' --exclude='*tests.rs'

fail_before_test_modules \
  'issue #382 architecture check: positional shipped definition lookup remains in production' \
  'shipped_agent_type\([0-9]+\)'
