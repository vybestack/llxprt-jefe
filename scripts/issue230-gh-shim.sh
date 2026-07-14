#!/usr/bin/env bash
# Fail-closed gh shim for issue #230 tmux scenario.
#
# Matches the COMPLETE Bash argv array against explicit exact production
# argument vectors — not parsed flags, substring markers, or prefix
# variables. Every accepted call logs ACCEPTED with its operation label;
# every rejected call logs REJECTED and exits non-zero.
#
# The exact operations for this scenario (in order):
#   1. GraphQL issue search list (initial load)
#   2. `gh issue view --json` detail read
#   3. GraphQL issue comments
#
# If production startup invokes `gh auth status`, its exact vector is also
# allowlisted. Every other invocation — mutations, reordered args, duplicate
# flags, wrong repo/issue/query/page size, marker-containing arbitrary
# GraphQL, extra args, auth trailing args — is REJECTED.
#
# The shim also generically allows a repeat of the search vector (operation
# #1) so a future scenario that refreshes the issues list can exercise it,
# but this scenario's driver never triggers one.
#
# This shim MUST NOT perform any live GitHub request. It never delegates to
# the real gh binary. It is a test-only fixture seam.
set -euo pipefail

if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 3))); then
    echo "gh shim: Bash 4.3 or newer is required" >&2
    exit 2
fi

if ! command -v flock >/dev/null 2>&1; then
    echo "gh shim: flock is required for audit logging" >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES="$SCRIPT_DIR/issue230-gh-shim-fixtures.sh"
if [[ ! -r "$FIXTURES" ]]; then
    echo "gh shim: shared fixtures file is missing or not readable: $FIXTURES" >&2
    exit 2
fi
# shellcheck source=issue230-gh-shim-fixtures.sh
. "$FIXTURES"

# Track whether this script created the audit file so the EXIT trap can
# clean it up. When a caller supplies GH_SHIM_AUDIT, the file is persistent
# and must NOT be removed.
SHIM_OWNED_AUDIT=false
if [[ -n "${GH_SHIM_AUDIT:-}" ]]; then
    AUDIT_FILE="$GH_SHIM_AUDIT"
else
    AUDIT_FILE=$(mktemp "${TMPDIR:-/tmp}/${ISSUE230_AUDIT_PREFIX}.XXXXXX.log") || {
        echo "gh shim: failed to create a private audit file" >&2
        exit 2
    }
    SHIM_OWNED_AUDIT=true
fi
if ! : 2>/dev/null >> "$AUDIT_FILE"; then
    echo "gh shim: audit file is not writable: $AUDIT_FILE" >&2
    exit 2
fi

# Remove the script-created audit file on exit. This trap is intentionally
# registered AFTER the audit file is opened and only removes it when
# SHIM_OWNED_AUDIT is true, so caller-supplied persistent files are never
# touched. The trap runs only when the script process exits — the fixture
# response is emitted by main() before any normal return, so cleanup cannot
# disrupt it.
cleanup_shim_audit() {
    if [[ "$SHIM_OWNED_AUDIT" == true && -n "${AUDIT_FILE:-}" && -f "$AUDIT_FILE" ]]; then
        rm -f "$AUDIT_FILE" 2>/dev/null || true
    fi
}
trap cleanup_shim_audit EXIT

audit_timestamp() {
    date -u +%Y%m%dT%H%M%SZ 2>/dev/null || echo "unknown"
}

# ─── Audit helpers ───────────────────────────────────────────────────────

audit_accept() {
    local op="$1"; shift
    audit_write "[$(audit_timestamp)] ACCEPTED $op -- gh $(shell_quote "$@")"
}

audit_reject() {
    local reason="$1"; shift
    audit_write "[$(audit_timestamp)] REJECTED $reason -- gh $(shell_quote "$@")"
}

audit_write() {
    local record="$1"
    local audit_fd
    if ! exec {audit_fd}>> "$AUDIT_FILE"; then
        echo "gh shim: audit file became unwritable: $AUDIT_FILE" >&2
        return 2
    fi
    if ! flock -w 5 -x "$audit_fd"; then
        echo "gh shim: timed out or failed while locking audit file: $AUDIT_FILE" >&2
        exec {audit_fd}>&-
        return 2
    fi
    if ! printf '%s\n' "$record" >&"$audit_fd"; then
        echo "gh shim: failed while writing audit file: $AUDIT_FILE" >&2
        flock -u "$audit_fd" || true
        exec {audit_fd}>&-
        return 2
    fi
    flock -u "$audit_fd" || true
    exec {audit_fd}>&-
}

shell_quote() {
    local out=""
    local first=1
    for arg in "$@"; do
        if [[ $first -eq 1 ]]; then
            first=0
        else
            out+=" "
        fi
        local quoted
        printf -v quoted '%q' "$arg"
        out+="$quoted"
    done
    printf '%s' "$out"
}

reject() {
    local reason="$1"
    shift
    # Audit logging is best-effort: if the audit file becomes unwritable or
    # un-lockable, the rejection must still reach the expected `exit 1` so
    # callers can distinguish a rejected command from an audit failure.
    audit_reject "$reason" "$@" || true
    printf 'gh shim: REJECTED (%s): gh %s
' "$reason" "$(shell_quote "$@")" >&2
    exit 1
}

# ─── Exact argv comparison ───────────────────────────────────────────────

argv_eq() {
    local -n _expected_ref="$1"
    local -n _actual_ref="$2"
    local expected_len=${#_expected_ref[@]}
    local actual_len=${#_actual_ref[@]}
    if [[ $expected_len -ne $actual_len ]]; then
        return 1
    fi
    local i
    for ((i = 0; i < expected_len; i++)); do
        if [[ "${_expected_ref[i]}" != "${_actual_ref[i]}" ]]; then
            return 1
        fi
    done
    return 0
}

# ─── Exact production argument vectors ───────────────────────────────────
#
# Each allowlisted vector below must match the exact argv produced by the
# corresponding Rust producer. If a production call changes, update both the
# Rust code and the matching vector here:
#
#   search       → build_issue_search_args()   in src/github/parse.rs
#   issue-view   → GhClient::get_issue_detail() in src/github/mod.rs
#                  (gh issue view --json ...)
#   comments     → GhClient::list_comments()    in src/github/mod.rs
#                  (gh api graphql with repository.issue.comments)
#   auth-status  → GhClient::check_auth()       in src/github/mod.rs
#                  (gh auth status)

build_search_argv() {
    SEARCH_ARGV=(
        "api"
        "graphql"
        "-f"
        "query=${ISSUE230_SEARCH_QUERY_BODY}"
        "-F"
        "searchQuery=${ISSUE230_SEARCH_QUERY_STRING}"
        "-F"
        "first=30"
    )
}

build_issue_view_argv() {
    ISSUE_VIEW_ARGV=(
        "issue"
        "view"
        "--repo"
        "${ISSUE230_REPO_SLUG}"
        "${ISSUE230_NUMBER}"
        "--json"
        "${ISSUE230_VIEW_JSON_FIELDS}"
    )
}

build_comments_argv() {
    COMMENTS_ARGV=(
        "api"
        "graphql"
        "-f"
        "query=${ISSUE230_COMMENTS_QUERY_BODY}"
        "-F"
        "owner=${ISSUE230_REPO_OWNER}"
        "-F"
        "repo=${ISSUE230_REPO_NAME}"
        "-F"
        "number=${ISSUE230_NUMBER}"
        "-F"
        "first=30"
    )
}

build_auth_status_argv() {
    AUTH_STATUS_ARGV=(
        "auth"
        "status"
    )
}

# ─── Fixture payloads ────────────────────────────────────────────────────

issue_search_json() {
    cat <<'EOF'
{"data":{"search":{"nodes":[{"id":"I_kwADOAAAABc230","number":230,"title":"Agent chooser identity and worktree status","state":"OPEN","author":{"login":"testuser"},"updatedAt":"2024-01-01T00:00:00Z","assignees":{"nodes":[]},"labels":{"nodes":[]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
EOF
}

issue_view_json() {
    cat <<'EOF'
{"number":230,"title":"Agent chooser identity and worktree status","state":"OPEN","author":{"login":"testuser"},"createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z","labels":{"nodes":[]},"assignees":{"nodes":[]},"milestone":null,"body":"Issue #230 detail body","url":"https://github.com/owner/repo-230/issues/230","id":"I_kwADOAAAABc230","comments":[]}
EOF
}

issue_comments_json() {
    cat <<'EOF'
{"data":{"repository":{"issue":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
EOF
}

# ─── Exact-match command routing ─────────────────────────────────────────

main() {
    local -a actual=("$@")

    [[ ${#actual[@]} -gt 0 ]] || {
        reject "no subcommand" ""
    }

    build_search_argv
    build_issue_view_argv
    build_comments_argv
    build_auth_status_argv

    if argv_eq SEARCH_ARGV actual; then
        audit_accept "search" "${actual[@]}" || true
        issue_search_json
        return 0
    fi

    if argv_eq ISSUE_VIEW_ARGV actual; then
        audit_accept "issue-view" "${actual[@]}" || true
        issue_view_json
        return 0
    fi

    if argv_eq COMMENTS_ARGV actual; then
        audit_accept "comments" "${actual[@]}" || true
        issue_comments_json
        return 0
    fi

    if argv_eq AUTH_STATUS_ARGV actual; then
        audit_accept "auth-status" "${actual[@]}" || true
        echo "github.com"
        echo "  Logged in to github.com account testuser"
        return 0
    fi

    reject "unmatched argv (not an exact allowlisted vector)" "${actual[@]}"
}

main "$@"
