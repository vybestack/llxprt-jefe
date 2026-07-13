#!/usr/bin/env bash
# Self-test for the issue #265 gh shim: proves exact argv matching.
#
# Constructs each exact production vector and asserts it PASSES, then
# constructs deliberate deviations (reordered args, duplicate flags, wrong
# repo/issue/query/page size, marker-containing arbitrary GraphQL, extra
# args, auth trailing args) and asserts they are REJECTED (non-zero exit).
#
# Every test invokes the actual shim binary with GH_SHIM_AUDIT pointed at a
# temp file so the real routing logic is exercised end-to-end.
#
# Usage: scripts/issue265-shim-selftest.sh
# Exit: 0 if all tests pass, 1 if any fail.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SHIM="$PROJECT_ROOT/scripts/issue265-gh-shim.sh"
FIXTURES="$SCRIPT_DIR/issue265-gh-shim-fixtures.sh"

[[ -x "$SHIM" ]] || {
    echo "FATAL: shim not found or not executable: $SHIM" >&2
    exit 1
}
[[ -r "$FIXTURES" ]] || {
    echo "FATAL: shared fixtures file is missing or not readable: $FIXTURES" >&2
    exit 1
}
command -v timeout >/dev/null 2>&1 || {
    echo "FATAL: timeout is required for the Linux shim self-test" >&2
    exit 1
}

# Source the shared readonly production-vector constants so the self-test
# and shim can never drift apart.
# shellcheck source=issue265-gh-shim-fixtures.sh
. "$FIXTURES"
for fixture_name in SEARCH_QUERY_BODY SEARCH_QUERY_STRING ISSUE_VIEW_JSON_FIELDS COMMENTS_QUERY_BODY; do
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
cp "$FIXTURES" "$DEPLOYED_SHIM_DIR/issue265-gh-shim-fixtures.sh"
chmod +x "$DEPLOYED_SHIM_DIR/gh"
SHIM="$DEPLOYED_SHIM_DIR/gh"
[[ -w "$TMPAUDIT" ]] || {
    echo "FATAL: audit temp file is not writable: $TMPAUDIT" >&2
    exit 1
}

# Run the shim with given args; capture output, audit, and bounded exit status.
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
    echo "FAIL (expected $expectation): $label"
    echo "  exit: $SHIM_EXIT"
    echo "  stdout: $SHIM_STDOUT"
    echo "  stderr: $SHIM_STDERR"
    echo "  audit: $SHIM_AUDIT"
}

# Return 0 only when the audit content is exactly one non-empty line.
# Rejects empty content and multi-line content. Avoids the fragile
# `grep -c '^'` here-string pattern which can exit 1 under `set -e` when
# the count is zero.
exact_one_nonempty_audit_record() {
    local content="${SHIM_AUDIT:-}"
    local stripped="${content%$'\n'}"
    if [[ -n "$stripped" && "$stripped" != *$'\n'* ]]; then
        return 0
    fi
    return 1
}

# Assert the shim ACCEPTS the given args and records the expected operation.
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

# Assert the shim REJECTS with the fail-closed audit reason.
expect_reject() {
    local label="$1"; shift
    run_shim "$@"
    if [[ $SHIM_EXIT -ne 0 && $SHIM_EXIT -ne 124 ]] \
        && exact_one_nonempty_audit_record \
        && [[ "$SHIM_AUDIT" == *"] REJECTED unmatched argv (not an exact allowlisted vector) -- gh"* ]]; then
        PASS=$((PASS + 1))
    else
        record_failure "REJECT" "$label"
    fi
}

echo "== issue #265 gh shim self-test =="

# ── POSITIVE: exact vectors must pass ────────────────────────────────────

expect_accept "search exact vector" "search" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=30"

expect_accept "issue-view exact vector" "issue-view" \
    issue view \
    --repo "owner/repo-265" \
    "265" \
    --json "${ISSUE_VIEW_JSON_FIELDS}"

expect_accept "comments exact vector" "comments" \
    api graphql \
    -f "query=${COMMENTS_QUERY_BODY}" \
    -F "owner=owner" \
    -F "repo=repo-265" \
    -F "number=265" \
    -F "first=30"

expect_accept "auth status exact vector" "auth-status" \
    auth status

# ── NEGATIVE: reordered args ─────────────────────────────────────────────

# issue-view with --json before the number (reordered).
expect_reject "issue-view reordered --json before number" \
    issue view \
    --repo "owner/repo-265" \
    --json "${ISSUE_VIEW_JSON_FIELDS}" \
    "265"

# search with -F flags before -f query (reordered).
expect_reject "search reordered vars before query" \
    api graphql \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=30" \
    -f "query=${SEARCH_QUERY_BODY}"

# comments with -F flags in different order.
expect_reject "comments reordered vars" \
    api graphql \
    -f "query=${COMMENTS_QUERY_BODY}" \
    -F "first=30" \
    -F "number=265" \
    -F "repo=repo-265" \
    -F "owner=owner"

# ── NEGATIVE: duplicate flags ────────────────────────────────────────────

expect_reject "search duplicate first flag" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=30" \
    -F "first=30"

expect_reject "issue-view duplicate --repo" \
    issue view \
    --repo "owner/repo-265" \
    --repo "owner/repo-265" \
    "265" \
    --json "${ISSUE_VIEW_JSON_FIELDS}"

# ── NEGATIVE: wrong repo ─────────────────────────────────────────────────

expect_reject "search wrong repo" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=repo:owner/wrong-repo is:issue state:open" \
    -F "first=30"

expect_reject "issue-view wrong repo" \
    issue view \
    --repo "owner/wrong-repo" \
    "265" \
    --json "${ISSUE_VIEW_JSON_FIELDS}"

expect_reject "comments wrong repo" \
    api graphql \
    -f "query=${COMMENTS_QUERY_BODY}" \
    -F "owner=owner" \
    -F "repo=wrong-repo" \
    -F "number=265" \
    -F "first=30"

# ── NEGATIVE: wrong issue number ─────────────────────────────────────────

expect_reject "issue-view wrong number" \
    issue view \
    --repo "owner/repo-265" \
    "999" \
    --json "${ISSUE_VIEW_JSON_FIELDS}"

expect_reject "comments wrong number" \
    api graphql \
    -f "query=${COMMENTS_QUERY_BODY}" \
    -F "owner=owner" \
    -F "repo=repo-265" \
    -F "number=999" \
    -F "first=30"

# ── NEGATIVE: wrong page size ────────────────────────────────────────────

expect_reject "search wrong page size" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=50"

expect_reject "comments wrong page size" \
    api graphql \
    -f "query=${COMMENTS_QUERY_BODY}" \
    -F "owner=owner" \
    -F "repo=repo-265" \
    -F "number=265" \
    -F "first=10"

# ── NEGATIVE: marker-containing arbitrary GraphQL ────────────────────────
#
# A query that contains the search markers but is NOT the exact production
# query body must be rejected.

expect_reject "search marker-containing arbitrary GraphQL" \
    api graphql \
    -f "query=query { search(type: ISSUE, query: \"anything\") { nodes { number } } }" \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=30"

expect_reject "comments marker-containing arbitrary GraphQL" \
    api graphql \
    -f "query=query { repository(owner: \"x\") { issue(number: 1) { comments(first: 1) { nodes { id } } } } }" \
    -F "owner=owner" \
    -F "repo=repo-265" \
    -F "number=265" \
    -F "first=30"

# ── NEGATIVE: extra args ─────────────────────────────────────────────────

expect_reject "search extra trailing arg" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=${SEARCH_QUERY_STRING}" \
    -F "first=30" \
    "--verbose"

expect_reject "issue-view extra trailing --web flag" \
    issue view \
    --repo "owner/repo-265" \
    "265" \
    --json "${ISSUE_VIEW_JSON_FIELDS}" \
    --web

# ── NEGATIVE: auth trailing args ─────────────────────────────────────────

expect_reject "auth status with trailing arg" \
    auth status --show-token

expect_reject "auth login (wrong auth subcommand)" \
    auth login --web

# ── NEGATIVE: mutations ──────────────────────────────────────────────────

expect_reject "issue create mutation" \
    issue create --repo "owner/repo-265" --title "test"

expect_reject "api POST mutation" \
    api --method POST "/repos/owner/repo-265/issues" -f "title=test"

# ── NEGATIVE: missing args ───────────────────────────────────────────────

expect_reject "search missing first var" \
    api graphql \
    -f "query=${SEARCH_QUERY_BODY}" \
    -F "searchQuery=${SEARCH_QUERY_STRING}"

expect_reject "issue-view missing --json" \
    issue view \
    --repo "owner/repo-265" \
    "265"

# ── Summary ──────────────────────────────────────────────────────────────

echo ""
echo "Self-test results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
exit 0
