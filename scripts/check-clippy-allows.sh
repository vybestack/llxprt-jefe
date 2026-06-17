#!/usr/bin/env bash
set -euo pipefail

readonly ALLOWLIST="clippy-allowlist.tsv"
readonly ROOT_CLIPPY_CONFIG="clippy.toml"
readonly CI_CLIPPY_CONFIG=".github/clippy/clippy.toml"
readonly TMP_BASE="${TMPDIR:-/tmp}"

errors=0

fail() {
  echo "ERROR: $*" >&2
  errors=$((errors + 1))
}

require_file() {
  local file="$1"
  if [ ! -f "$file" ]; then
    fail "required file is missing: $file"
    return 1
  fi
}

make_temp_file() {
  mktemp "${TMP_BASE%/}/jefe-clippy-allows.XXXXXX"
}

actual_file="$(make_temp_file)"
expected_file="$(make_temp_file)"
trap 'rm -f "$actual_file" "$expected_file"' EXIT

scan_actual_allows() {
  git ls-files --cached '*.rs' ':!vendor/**' \
    | while IFS= read -r file; do
        awk -v file="$file" '
          function trim(value) {
            sub(/^[[:space:]]+/, "", value)
            sub(/[[:space:]]+$/, "", value)
            return value
          }
          function emit_attr() {
            attr = trim(buffer)
            gsub(/[[:space:]]+/, " ", attr)
            if (attr ~ /^#!?\[(cfg_attr\(.*clippy::|allow\(.*clippy::)/) {
              print file "\t" attr
            }
            buffer = ""
            collecting = 0
          }
          {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            if (!collecting && line ~ /^#!?\[(cfg_attr\(|allow\()/) {
              buffer = line
              collecting = 1
              if (line ~ /\]/) {
                emit_attr()
              }
              next
            }
            if (collecting) {
              buffer = buffer " " line
              if (line ~ /\]/) {
                emit_attr()
              }
            }
          }
        ' "$file"
      done \
    | LC_ALL=C sort
}

load_expected_allows() {
  awk -F '\t' '
    /^#/ || /^[[:space:]]*$/ { next }
    NF != 4 {
      printf "ERROR: malformed allowlist row %d: expected 4 tab-separated columns\n", NR > "/dev/stderr"
      bad = 1
      next
    }
    $1 == "" || $2 == "" || $3 == "" || $4 == "" {
      printf "ERROR: malformed allowlist row %d: empty columns are not permitted\n", NR > "/dev/stderr"
      bad = 1
      next
    }
    { print $1 "\t" $2 }
    END { exit bad }
  ' "$ALLOWLIST" | LC_ALL=C sort
}

clippy_config_value() {
  local file="$1"
  local key="$2"
  local line

  line="$(grep -E "^[[:space:]]*${key}[[:space:]]*=" "$file" || true)"
  if [ -z "$line" ]; then
    fail "$file is missing clippy threshold: $key"
    return 1
  fi

  printf '%s\n' "$line" \
    | head -n 1 \
    | sed -E 's/^[^=]+=[[:space:]]*//; s/[[:space:]]*(#.*)?$//'
}

check_allowlist() {
  require_file "$ALLOWLIST" || return

  scan_actual_allows > "$actual_file"
  if ! load_expected_allows > "$expected_file"; then
    errors=$((errors + 1))
    return
  fi

  if ! diff -u "$expected_file" "$actual_file"; then
    fail "clippy allow attributes must match $ALLOWLIST; remove the allow or update the approved exception record"
  fi
}

check_clippy_config_sync() {
  require_file "$ROOT_CLIPPY_CONFIG" || return
  require_file "$CI_CLIPPY_CONFIG" || return

  local thresholds=(
    cognitive-complexity-threshold
    too-many-lines-threshold
    too-many-arguments-threshold
    max-struct-bools
    type-complexity-threshold
  )

  local key root_value ci_value
  for key in "${thresholds[@]}"; do
    root_value="$(clippy_config_value "$ROOT_CLIPPY_CONFIG" "$key" || true)"
    ci_value="$(clippy_config_value "$CI_CLIPPY_CONFIG" "$key" || true)"
    if [ -n "$root_value" ] && [ -n "$ci_value" ] && [ "$root_value" != "$ci_value" ]; then
      fail "clippy threshold mismatch for $key: $ROOT_CLIPPY_CONFIG=$root_value, $CI_CLIPPY_CONFIG=$ci_value"
    fi
  done
}

check_allowlist
check_clippy_config_sync

if [ "$errors" -gt 0 ]; then
  echo "Clippy allow policy failed with $errors error(s)." >&2
  exit 1
fi

echo "Clippy allow policy passed."
