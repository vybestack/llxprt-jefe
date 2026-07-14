#!/usr/bin/env bash
# Enforce the source file length policy.
#
# Fails when a Rust source file under the scan roots exceeds the hard line
# limit, and warns above a recommended limit. This is the local equivalent of
# the `source_file_size` CI job in .github/workflows/ci.yml; `make ci-check`
# runs it so contributors can reproduce that gate before pushing.
#
# Usage:
#   scripts/check-source-file-size.sh
#
# Overrides (optional):
#   SCAN_ROOTS  space-separated roots to scan (default: "src tests")
#   HARD_LIMIT  hard failure limit, in lines (default: 1000)
#   WARN_LIMIT  recommended limit, in lines (default: 750)
set -euo pipefail

# Split the override into an array so multiple roots are handled by quoting
# rather than unquoted word-splitting (which would also risk glob expansion).
read -r -a SCAN_ROOTS <<< "${SCAN_ROOTS:-src tests tools}"
readonly SCAN_ROOTS
readonly HARD_LIMIT="${HARD_LIMIT:-1000}"
readonly WARN_LIMIT="${WARN_LIMIT:-750}"

errors=0
warnings=0
found=0

fail() {
  echo "ERROR: $*" >&2
  errors=$((errors + 1))
}

warn() {
  echo "WARNING: $*" >&2
  warnings=$((warnings + 1))
}

# Process substitution keeps the loop in this shell so the counters persist.
while IFS= read -r file; do
  found=1
  lines=$(wc -l < "$file" | tr -d '[:space:]')
  if [ "$lines" -gt "$HARD_LIMIT" ]; then
    fail "$file has $lines lines (max $HARD_LIMIT)"
  elif [ "$lines" -gt "$WARN_LIMIT" ]; then
    warn "$file has $lines lines (recommended max $WARN_LIMIT)"
  fi
done < <(find "${SCAN_ROOTS[@]}" -type f -name '*.rs' 2>/dev/null | sort)

if [ "$found" -eq 0 ]; then
  echo "No Rust source files found under: ${SCAN_ROOTS[*]}"
  exit 0
fi

if [ "$warnings" -gt 0 ]; then
  echo "Emitted $warnings file length warning(s)." >&2
fi

if [ "$errors" -gt 0 ]; then
  echo "Found $errors file(s) exceeding the hard limit." >&2
  exit 1
fi
