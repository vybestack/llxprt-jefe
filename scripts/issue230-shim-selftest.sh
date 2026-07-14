#!/usr/bin/env bash
# Self-test for the issue #230 gh shim: proves exact argv matching.
#
# Constructs each exact production vector and asserts it PASSES, then
# constructs deliberate deviations (reordered args, duplicate flags, wrong
# repo/issue/query/page size, marker-containing arbitrary GraphQL, extra
# args, auth trailing args) and asserts they are REJECTED (non-zero exit).
#
# Every test invokes the actual shim binary with GH_SHIM_AUDIT pointed at a
# temp file so the real routing logic is exercised end-to-end.
#
# Usage: scripts/issue230-shim-selftest.sh
# Exit: 0 if all tests pass, 1 if any fail.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SHIM="$PROJECT_ROOT/scripts/issue230-gh-shim.sh"
FIXTURES="$SCRIPT_DIR/issue230-gh-shim-fixtures.sh"

[[ -x "$SHIM" ]] || {
    echo "FATAL: shim not found or not executable: $SHIM" >&2
    exit 1
}
[[ -r "$FIXTURES" ]] || {
    echo "FATAL: shared fixtures file is missing or not readable: $FIXTURES" >&2
    exit 1
}
command -v timeout >/dev/null 2>&1 || {
    echo "FATAL: timeout is required for the shim self-test" >&2
    exit 1
}
# Verify GNU timeout behavior (exit 124 on timeout). BSD timeout (macOS) and
# some busybox implementations have different exit-code semantics. The
# self-test uses timeout to guard against hangs and relies on exit 124.
timeout_status=0
timeout 0.1s sleep 1 2>/dev/null || timeout_status=$?
if [[ $timeout_status -ne 124 ]]; then
    echo "FATAL: timeout does not behave like GNU coreutils timeout (exit code $timeout_status, expected 124)" >&2
    exit 1
fi

# shellcheck source=issue230-gh-shim-fixtures.sh
. "$FIXTURES"
for fixture_name in ISSUE230_SEARCH_QUERY_BODY ISSUE230_SEARCH_QUERY_STRING ISSUE230_VIEW_JSON_FIELDS ISSUE230_COMMENTS_QUERY_BODY ISSUE230_REPO_SLUG ISSUE230_NUMBER ISSUE230_REPO_OWNER ISSUE230_REPO_NAME; do
    fixture_declaration=$(declare -p "$fixture_name" 2>/dev/null) || {
        echo "FATAL: shared fixture is not declared: $fixture_name" >&2
        exit 1
    }
    [[ "$fixture_declaration" == "declare -r "* ]] || {
        echo "FATAL: shared fixture is not readonly: $fixture_name" >&2
        exit 1
    }
    [[ -n "${!fixture_name}" ]] || {
        echo "FATAL: shared fixture is empty: $fixture_name" >&2
        exit 1
    }
done

PASS=0
FAIL=0
TMPAUDIT=$(mktemp)
TMPSTDERR=$(mktemp)
DEPLOYED_SHIM_DIR=$(mktemp -d)
trap 'rm -f "$TMPAUDIT" "$TMPSTDERR"; rm -rf "$DEPLOYED_SHIM_DIR"' EXIT
cp "$SHIM" "$DEPLOYED_SHIM_DIR/gh"
cp "$FIXTURES" "$DEPLOYED_SHIM_DIR/issue230-gh-shim-fixtures.sh"
chmod +x "$DEPLOYED_SHIM_DIR/gh"
SHIM="$DEPLOYED_SHIM_DIR/gh"
[[ -w "$TMPAUDIT" ]] || {
    echo "FATAL: audit temp file is not writable: $TMPAUDIT" >&2
    exit 1
}

run_shim() {
    export GH_SHIM_AUDIT="$TMPAUDIT"
    : > "$TMPAUDIT"
    : > "$TMPSTDERR"
    SHIM_STDOUT=""
    SHIM_STDERR=""
    SHIM_AUDIT=""
    SHIM_EXIT=0
    SHIM_STDOUT=$(timeout 10s "$SHIM" "$@" 2>"$TMPSTDERR") || SHIM_EXIT=$?
    SHIM_STDERR=$(cat "$TMPSTDERR")
    SHIM_AUDIT=$(cat "$TMPAUDIT")
}

record_failure() {
    local expectation="$1"
    local label="$2"
    FAIL=$((FAIL + 1))
    printf 'FAIL (expected %s): %s
' "$expectation" "$label"
    printf '  exit: %s
' "$SHIM_EXIT"
    printf '  stdout: %s
' "$SHIM_STDOUT"
    printf '  stderr: %s
' "$SHIM_STDERR"
    printf '  audit: %s
' "$SHIM_AUDIT"
}

exact_one_nonempty_audit_record() {
    local content="${SHIM_AUDIT:-}"
    local stripped="${content%$'\n'}"
    if [[ -n "$stripped" && "$stripped" != *$'\n'* ]]; then
        return 0
    fi
    return 1
}

expect_accept() {
    local label="$1"
    local operation="$2"
    shift 2
    run_shim "$@"
    if [[ $SHIM_EXIT -eq 0 ]] \
        && exact_one_nonempty_audit_record \
        && [[ "$SHIM_AUDIT" == *"] ACCEPTED $operation -- gh "* ]]; then
        PASS=$((PASS + 1))
    else
        record_failure "ACCEPT ($operation)" "$label"
    fi
}

expect_reject() {
    local label="$1"; shift
    run_shim "$@"
    if [[ $SHIM_EXIT -eq 124 ]]; then
        FAIL=$((FAIL + 1))
        printf 'FAIL (expected REJECT but shim TIMED OUT): %s\n' "$label"
        printf '  exit: %s (timeout)\n' "$SHIM_EXIT"
        printf '  stdout: %s\n' "$SHIM_STDOUT"
        printf '  stderr: %s\n' "$SHIM_STDERR"
        printf '  audit: %s\n' "$SHIM_AUDIT"
        return
    fi
    if [[ $SHIM_EXIT -ne 0 ]] \
        && exact_one_nonempty_audit_record \
        && [[ "$SHIM_AUDIT" == *"] REJECTED "* ]] \
        && [[ "$SHIM_STDERR" == *"REJECTED"* ]]; then
        PASS=$((PASS + 1))
    else
        record_failure "REJECT" "$label"
    fi
}

echo "== issue #230 gh shim self-test =="

# ── POSITIVE: exact vectors must pass ────────────────────────────────────

expect_accept "search exact vector" "search" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=30"

expect_accept "issue-view exact vector" "issue-view" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    "${ISSUE230_NUMBER}" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}"

expect_accept "comments exact vector" "comments" \
    api graphql \
    -f "query=${ISSUE230_COMMENTS_QUERY_BODY}" \
    -F "owner=${ISSUE230_REPO_OWNER}" \
    -F "repo=${ISSUE230_REPO_NAME}" \
    -F "number=${ISSUE230_NUMBER}" \
    -F "first=30"

expect_accept "auth status exact vector" "auth-status" \
    auth status

# ── NEGATIVE: reordered args ─────────────────────────────────────────────

expect_reject "issue-view reordered --json before number" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}" \
    "${ISSUE230_NUMBER}"

expect_reject "search reordered vars before query" \
    api graphql \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=30" \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}"

expect_reject "comments reordered vars" \
    api graphql \
    -f "query=${ISSUE230_COMMENTS_QUERY_BODY}" \
    -F "first=30" \
    -F "number=${ISSUE230_NUMBER}" \
    -F "repo=${ISSUE230_REPO_NAME}" \
    -F "owner=${ISSUE230_REPO_OWNER}"

# ── NEGATIVE: duplicate flags ────────────────────────────────────────────

expect_reject "search duplicate first flag" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=30" \
    -F "first=30"

expect_reject "issue-view duplicate --repo" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    --repo "${ISSUE230_REPO_SLUG}" \
    "${ISSUE230_NUMBER}" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}"

# ── NEGATIVE: wrong repo ─────────────────────────────────────────────────

expect_reject "search wrong repo" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=repo:owner/wrong-repo is:issue state:open" \
    -F "first=30"

expect_reject "issue-view wrong repo" \
    issue view \
    --repo "owner/wrong-repo" \
    "${ISSUE230_NUMBER}" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}"

expect_reject "comments wrong repo" \
    api graphql \
    -f "query=${ISSUE230_COMMENTS_QUERY_BODY}" \
    -F "owner=${ISSUE230_REPO_OWNER}" \
    -F "repo=wrong-repo" \
    -F "number=${ISSUE230_NUMBER}" \
    -F "first=30"

# ── NEGATIVE: wrong issue number ─────────────────────────────────────────

expect_reject "issue-view wrong number" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    "999" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}"

expect_reject "comments wrong number" \
    api graphql \
    -f "query=${ISSUE230_COMMENTS_QUERY_BODY}" \
    -F "owner=${ISSUE230_REPO_OWNER}" \
    -F "repo=${ISSUE230_REPO_NAME}" \
    -F "number=999" \
    -F "first=30"

# ── NEGATIVE: wrong page size ────────────────────────────────────────────

expect_reject "search wrong page size" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=50"

expect_reject "comments wrong page size" \
    api graphql \
    -f "query=${ISSUE230_COMMENTS_QUERY_BODY}" \
    -F "owner=${ISSUE230_REPO_OWNER}" \
    -F "repo=${ISSUE230_REPO_NAME}" \
    -F "number=${ISSUE230_NUMBER}" \
    -F "first=10"

# ── NEGATIVE: marker-containing arbitrary GraphQL ────────────────────────

expect_reject "search marker-containing arbitrary GraphQL" \
    api graphql \
    -f "query=query { search(type: ISSUE, query: \"anything\") { nodes { number } } }" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=30"

expect_reject "comments marker-containing arbitrary GraphQL" \
    api graphql \
    -f "query=query { repository(owner: \"x\") { issue(number: 1) { comments(first: 1) { nodes { id } } } } }" \
    -F "owner=${ISSUE230_REPO_OWNER}" \
    -F "repo=${ISSUE230_REPO_NAME}" \
    -F "number=${ISSUE230_NUMBER}" \
    -F "first=30"

# ── NEGATIVE: extra args ─────────────────────────────────────────────────

expect_reject "search extra trailing arg" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}" \
    -F "first=30" \
    "--verbose"

expect_reject "issue-view extra trailing --web flag" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    "${ISSUE230_NUMBER}" \
    --json "${ISSUE230_VIEW_JSON_FIELDS}" \
    --web

# ── NEGATIVE: auth trailing args ─────────────────────────────────────────

expect_reject "auth status with trailing arg" \
    auth status --show-token

expect_reject "auth login (wrong auth subcommand)" \
    auth login --web

# ── NEGATIVE: mutations ──────────────────────────────────────────────────

expect_reject "issue create mutation" \
    issue create --repo "${ISSUE230_REPO_SLUG}" --title "test"

expect_reject "api POST mutation" \
    api --method POST "/repos/${ISSUE230_REPO_SLUG}/issues" -f "title=test"

# ── NEGATIVE: missing args ───────────────────────────────────────────────
#
# The no-arguments case must be rejected with a non-zero, non-timeout exit
# code, a REJECTED audit record, and a REJECTED message on stderr. We do NOT
# assert on the exact empty-argv shell rendering (`-- gh ''`) because that is
# a brittle internal detail of the shim's shell_quote implementation. The
# security-critical contract is: reject + audit + stderr message.

assert_rejected_audit_and_stderr() {
    local label="$1"
    if [[ $SHIM_EXIT -eq 124 ]]; then
        FAIL=$((FAIL + 1))
        printf 'FAIL (expected REJECT but shim TIMED OUT): %s\n' "$label"
        printf '  exit: %s (timeout)\n' "$SHIM_EXIT"
        printf '  stdout: %s\n' "$SHIM_STDOUT"
        printf '  stderr: %s\n' "$SHIM_STDERR"
        printf '  audit: %s\n' "$SHIM_AUDIT"
        return
    fi
    if [[ $SHIM_EXIT -ne 0 ]] \
        && exact_one_nonempty_audit_record \
        && [[ "$SHIM_AUDIT" == *"] REJECTED "* ]] \
        && [[ "$SHIM_STDERR" == *"REJECTED"* ]]; then
        PASS=$((PASS + 1))
    else
        record_failure "REJECT" "$label"
    fi
}

run_shim
assert_rejected_audit_and_stderr "no arguments"

expect_reject "search missing first var" \
    api graphql \
    -f "query=${ISSUE230_SEARCH_QUERY_BODY}" \
    -F "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}"

expect_reject "issue-view missing --json" \
    issue view \
    --repo "${ISSUE230_REPO_SLUG}" \
    "${ISSUE230_NUMBER}"

# ── Audit cleanup: script-owned file removed on exit ─────────────────────
#
# When GH_SHIM_AUDIT is NOT supplied, the shim creates its own temp audit
# file and must remove it on exit. When GH_SHIM_AUDIT IS supplied, the file
# is persistent and must be preserved.
#
# Case 1 uses a DEDICATED temporary directory so the leftover-file assertion
# is race-free: no concurrent process (shim, test, or other) can create
# files in this isolated directory. The shim is run in a subshell with
# GH_SHIM_AUDIT unset and TMPDIR set to the isolated directory, so any
# script-owned audit file lands there. After the shim exits, we assert the
# directory contains NO leftover audit file.

CLEANUP_TEST_DIR=$(mktemp -d)
trap 'rm -f "$TMPAUDIT" "$TMPSTDERR"; rm -rf "$DEPLOYED_SHIM_DIR" "$CLEANUP_TEST_DIR"' EXIT

# Case 1: no GH_SHIM_AUDIT → temp file created and cleaned up.
# Run in a subshell with GH_SHIM_AUDIT explicitly unset (not `env -u`, which
# is not portable to all shells/cores) and TMPDIR pointed at the isolated
# directory. The shim creates its own audit file inside TMPDIR.
(
    unset GH_SHIM_AUDIT
    export TMPDIR="$CLEANUP_TEST_DIR"
    timeout 10s "$SHIM" auth status 2>/dev/null
) || true

# After exit, the isolated directory must contain no leftover owned audit
# file (the only file that could exist is one the shim failed to clean up).
leftover_count=$(find "$CLEANUP_TEST_DIR" -maxdepth 1 -name 'jefe-issue230-gh-audit.*' -type f 2>/dev/null | wc -l | tr -d ' ')
if [[ "$leftover_count" -eq 0 ]]; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
    printf 'FAIL: script-owned audit file was not cleaned up (%d leftover in %s)
' \
        "$leftover_count" "$CLEANUP_TEST_DIR" >&2
fi

# Case 2: GH_SHIM_AUDIT supplied → caller file preserved after exit.
persistent_audit=$(mktemp)
: > "$persistent_audit"
GH_SHIM_AUDIT="$persistent_audit" timeout 10s "$SHIM" auth status 2>/dev/null || true
if [[ -f "$persistent_audit" && -s "$persistent_audit" ]]; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
    printf 'FAIL: caller-supplied audit file was removed or empty
' >&2
fi
rm -f "$persistent_audit"

# ── Summary ──────────────────────────────────────────────────────────────

echo ""
echo "Self-test results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
exit 0
