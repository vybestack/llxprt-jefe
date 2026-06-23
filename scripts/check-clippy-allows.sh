#!/usr/bin/env bash
# Zero-tolerance clippy allow policy.
#
# This gate fails if any tracked first-party Rust file (outside vendor/)
# contains a clippy allow attribute. There is no exception ledger. If an
# exception is genuinely required, it must be raised as a design discussion,
# not committed as debt.
#
# It also verifies that the root clippy.toml and the CI clippy.toml keep the
# same complexity thresholds, so CLIPPY_CONF_DIR does not silently fall back
# to clippy defaults.
set -euo pipefail

readonly ROOT_CLIPPY_CONFIG="clippy.toml"
readonly CI_CLIPPY_CONFIG=".github/clippy/clippy.toml"

# When set, scan that directory tree for *.rs files instead of the
# git-tracked first-party set. This exists for fixture-based tests.
readonly SCAN_ROOT="${CLIPPY_ALLOW_SCAN_ROOT:-}"

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

# Print clippy allow attributes found in the given Rust file, one per line as
# "<file>\t<attribute>". Handles single-line and multi-line attributes and the
# three attribute shapes the scanner must catch:
#   #[allow(clippy::...)]
#   #![allow(clippy::...)]
#   #[cfg_attr(..., allow(clippy::...))]
#
# Robustness: the scanner uses a small Rust-aware lexer to collect attribute
# blocks beginning with `#[`, `#![`, `# [`, or `#! [` while ignoring comments
# and string/char literals. It tracks nested brackets so a `]` inside a string
# literal (for example `doc = "]"`) cannot end the attribute early. Before
# matching, comments and literals inside the collected attribute are stripped,
# internal whitespace is normalized, and clippy paths are matched with optional
# raw-identifier prefixes and optional whitespace around `::`.
scan_file() {
  local file="$1"
  python3 - "$file" <<'PY'
import re
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    source = handle.read()

CLIPPY_ALLOW = re.compile(r"\ballow\s*\([^)]*(?:r#)?clippy\s*::")


def skip_line_comment(text, index):
    newline = text.find("\n", index + 2)
    return len(text) if newline == -1 else newline + 1


def skip_block_comment(text, index):
    depth = 1
    index += 2
    while index < len(text) and depth > 0:
        if text.startswith("/*", index):
            depth += 1
            index += 2
        elif text.startswith("*/", index):
            depth -= 1
            index += 2
        else:
            index += 1
    return index


def skip_string(text, index):
    index += 1
    while index < len(text):
        if text[index] == "\\":
            index += 2
        elif text[index] == '"':
            return index + 1
        else:
            index += 1
    return index


def skip_char_literal(text, index):
    # Only skip actual char literals. Lifetimes such as `'a` are tokens too,
    # but they are not quoted strings; treating them as unterminated strings
    # would skip the rest of the file and hide later attributes.
    cursor = index + 1
    if cursor >= len(text):
        return index + 1
    if text[cursor] == "\\":
        cursor += 2
    else:
        cursor += 1
    return cursor + 1 if cursor < len(text) and text[cursor] == "'" else index + 1


def skip_raw_string(text, index):
    start = index
    if text.startswith("br", index):
        index += 2
    elif text.startswith("r", index):
        index += 1
    else:
        return start

    hashes = 0
    while index < len(text) and text[index] == "#":
        hashes += 1
        index += 1
    if index >= len(text) or text[index] != '"':
        return start

    terminator = '"' + ("#" * hashes)
    end = text.find(terminator, index + 1)
    return len(text) if end == -1 else end + len(terminator)


def sanitize(text):
    output = []
    index = 0
    while index < len(text):
        if text.startswith("//", index):
            output.append(" ")
            index = skip_line_comment(text, index)
        elif text.startswith("/*", index):
            output.append(" ")
            index = skip_block_comment(text, index)
        else:
            raw_end = skip_raw_string(text, index)
            if raw_end != index:
                output.append(" ")
                index = raw_end
            elif text[index] == '"':
                output.append(" ")
                index = skip_string(text, index)
            elif text[index] == "'":
                output.append(" ")
                index = skip_char_literal(text, index)
            else:
                output.append(text[index])
                index += 1
    return re.sub(r"\s+", " ", "".join(output).strip())


def collect_attribute(text, start):
    index = start + 1
    while index < len(text) and text[index].isspace():
        index += 1
    if index < len(text) and text[index] == "!":
        index += 1
        while index < len(text) and text[index].isspace():
            index += 1
    if index >= len(text) or text[index] != "[":
        return None

    bracket_depth = 1
    index += 1
    while index < len(text) and bracket_depth > 0:
        if text.startswith("//", index):
            index = skip_line_comment(text, index)
        elif text.startswith("/*", index):
            index = skip_block_comment(text, index)
        else:
            raw_end = skip_raw_string(text, index)
            if raw_end != index:
                index = raw_end
            elif text[index] == '"':
                index = skip_string(text, index)
            elif text[index] == "'":
                index = skip_char_literal(text, index)
            elif text[index] == "[":
                bracket_depth += 1
                index += 1
            elif text[index] == "]":
                bracket_depth -= 1
                index += 1
            else:
                index += 1

    return text[start:index] if bracket_depth == 0 else None


index = 0
while index < len(source):
    if source.startswith("//", index):
        index = skip_line_comment(source, index)
    elif source.startswith("/*", index):
        index = skip_block_comment(source, index)
    else:
        raw_end = skip_raw_string(source, index)
        if raw_end != index:
            index = raw_end
        elif source[index] == '"':
            index = skip_string(source, index)
        elif source[index] == "'":
            index = skip_char_literal(source, index)
        elif source[index] == "#":
            attribute = collect_attribute(source, index)
            if attribute is None:
                index += 1
            else:
                normalized = sanitize(attribute)
                if CLIPPY_ALLOW.search(normalized):
                    print(f"{path}\t{normalized}")
                index += len(attribute)
        else:
            index += 1
PY
}

# Emit one "<file>\t<attribute>" record per clippy allow attribute found.
#
# Fail closed: the function returns nonzero if the underlying file enumeration
# (git or find) or any per-file scan fails. This is critical — a scanner
# failure must NOT be reported as a clean (empty) result, or the zero-
# tolerance gate would be bypassed. Callers capture the exit status and treat
# a nonzero status as a policy failure.
scan_allows() {
  local file_list

  if [ -n "$SCAN_ROOT" ]; then
    if [ ! -d "$SCAN_ROOT" ]; then
      echo "ERROR: CLIPPY_ALLOW_SCAN_ROOT does not exist: $SCAN_ROOT" >&2
      return 1
    fi
    # Capture the file list into a variable so a find failure (nonzero exit)
    # propagates via pipefail rather than being hidden by process
    # substitution.
    file_list="$(find "$SCAN_ROOT" -type f -name '*.rs' | sort)" || return 1
  else
    file_list="$(git ls-files --cached '*.rs' ':!vendor/**')" || return 1
  fi

  local status=0
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    scan_file "$file" || status=1
  done <<<"$file_list"
  return "$status"
}

check_no_allows() {
  local found scan_status
  # Fail closed: capture the scan exit status explicitly. We must NOT mask a
  # scanner failure with `|| true`, because that would silently let real
  # errors (e.g. git failure, missing files) pass the gate. If the scanner
  # itself errors, the policy must fail rather than report a clean result.
  set +e
  found="$(scan_allows)"
  scan_status=$?
  set -e
  if [ "$scan_status" -ne 0 ]; then
    fail "clippy allow scanner failed (exit $scan_status); cannot verify policy — failing closed"
    return 1
  fi
  if [ -n "$found" ]; then
    fail "first-party clippy allow attributes are forbidden; remove them:"
    while IFS= read -r line; do
      printf '  %s\n' "$line" >&2
    done <<<"$found"
  fi
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

check_clippy_config_sync() {
  # Fixture mode skips the root/CI sync check (fixtures do not replicate the
  # repo layout).
  [ -n "$SCAN_ROOT" ] && return 0

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

check_no_allows
check_clippy_config_sync

if [ "$errors" -gt 0 ]; then
  echo "Clippy allow policy failed with $errors error(s)." >&2
  exit 1
fi

echo "Clippy allow policy passed."
