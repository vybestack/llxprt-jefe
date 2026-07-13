#!/usr/bin/env bash
# Fail-closed gh shim for issue #265 tmux scenario.
#
# Matches the COMPLETE Bash argv array against explicit exact production
# argument vectors — not parsed flags, substring markers, or prefix
# variables. Every accepted call logs ACCEPTED with its operation label;
# every rejected call logs REJECTED and exits non-zero.
#
# The four exact operations (in scenario order):
#   1. GraphQL issue search list (initial load)
#   2. `gh issue view --json` detail read
#   3. GraphQL issue comments
#   4. GraphQL issue search list (refresh)
#
# If production startup invokes `gh auth status`, its exact vector is also
# allowlisted. Every other invocation — mutations, reordered args, duplicate
# flags, wrong repo/issue/query/page size, marker-containing arbitrary
# GraphQL, extra args, auth trailing args — is REJECTED.
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

AUDIT_FILE="${GH_SHIM_AUDIT:-/tmp/jefe-issue265-gh-audit.log}"
if ! : 2>/dev/null >> "$AUDIT_FILE"; then
    echo "gh shim: audit file is not writable: $AUDIT_FILE" >&2
    exit 2
fi

audit_timestamp() {
    date -u +%Y%m%dT%H%M%SZ 2>/dev/null || echo "unknown"
}

# ─── Audit helpers ───────────────────────────────────────────────────────
#
# Every call is audited with its ACTUAL argv boundaries preserved (shell-
# quoted so a multi-word arg stays one token), never a canonical
# reconstruction.

audit_accept() {
    # $1 = operation label, $2.. = the original gh args (shell-quoted)
    local op="$1"; shift
    audit_write "[$(audit_timestamp)] ACCEPTED $op -- gh $(shell_quote "$@")"
}

audit_reject() {
    # $1 = reason, $2.. = the original gh args (shell-quoted)
    local reason="$1"; shift
    audit_write "[$(audit_timestamp)] REJECTED $reason -- gh $(shell_quote "$@")"
}

# Issue detail and comments reads may run concurrently in separate gh
# processes, so serialize each complete record.
audit_write() {
    local record="$1"
    local audit_fd
    exec {audit_fd}>> "$AUDIT_FILE"
    if ! flock -w 5 -x "$audit_fd"; then
        echo "gh shim: timed out or failed while locking audit file: $AUDIT_FILE" >&2
        exec {audit_fd}>&-
        exit 2
    fi
    printf '%s\n' "$record" >&"$audit_fd"
    flock -u "$audit_fd"
    exec {audit_fd}>&-
}

# Shell-quote each argument so multi-word tokens (e.g. the GraphQL query body)
# are preserved as single argv elements in the audit log.
shell_quote() {
    local out=""
    local first=1
    for arg in "$@"; do
        if [[ $first -eq 1 ]]; then
            first=0
        else
            out+=" "
        fi
        # Use printf %q for safe shell quoting of each individual argv element.
        local quoted
        printf -v quoted '%q' "$arg"
        out+="$quoted"
    done
    printf '%s' "$out"
}

reject() {
    audit_reject "$@"
    echo "gh shim: REJECTED ($1): gh ${*:2}" >&2
    exit 1
}

# ─── Exact argv comparison ───────────────────────────────────────────────
#
# Compare the actual argv array against an expected array element-by-element.
# Returns 0 (match) or 1 (mismatch). This is the ONLY matching mechanism —
# no flag parsing, no substring checks, no prefix variables.

# Usage: argv_eq EXPECTED_ARRAY_NAME ACTUAL_ARRAY_NAME
# Both must be bash arrays passed by name.
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
# These are extracted verbatim from the production source code for the
# issue #265 scenario (owner=owner, repo=repo-265, number=265, filter=default
# open, page_size=30, no cursor).

# The exact GraphQL search-list query body (no cursor — first page).
# Source: src/github/parse.rs build_issue_search_args, cursor=None branch.
readonly SEARCH_QUERY_BODY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on Issue { id number title state author { login } updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } } } pageInfo { hasNextPage endCursor } } }'

# The exact search query string for the default open filter on owner/repo-265.
readonly SEARCH_QUERY_STRING='repo:owner/repo-265 is:issue state:open'

# The exact --json field list for `gh issue view`.
# Source: src/github/mod.rs get_issue_detail.
readonly ISSUE_VIEW_JSON_FIELDS='number,title,state,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id'

# The exact GraphQL comments query body (no cursor — first page).
# Source: src/github/mod.rs list_comments, cursor=None branch.
readonly COMMENTS_QUERY_BODY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }'

# Build the exact expected argv for each operation. These are constructed
# from the readonly constants above so they cannot drift.

build_search_argv() {
    # gh api graphql -f query=<body> -F searchQuery=<query> -F first=30
    SEARCH_ARGV=(
        "api"
        "graphql"
        "-f"
        "query=${SEARCH_QUERY_BODY}"
        "-F"
        "searchQuery=${SEARCH_QUERY_STRING}"
        "-F"
        "first=30"
    )
}

build_issue_view_argv() {
    # gh issue view --repo owner/repo-265 265 --json <fields>
    ISSUE_VIEW_ARGV=(
        "issue"
        "view"
        "--repo"
        "owner/repo-265"
        "265"
        "--json"
        "${ISSUE_VIEW_JSON_FIELDS}"
    )
}

build_comments_argv() {
    # gh api graphql -f query=<body> -F owner=owner -F repo=repo-265 -F number=265 -F first=30
    COMMENTS_ARGV=(
        "api"
        "graphql"
        "-f"
        "query=${COMMENTS_QUERY_BODY}"
        "-F"
        "owner=owner"
        "-F"
        "repo=repo-265"
        "-F"
        "number=265"
        "-F"
        "first=30"
    )
}

build_auth_status_argv() {
    # gh auth status — exact two-word vector.
    AUTH_STATUS_ARGV=(
        "auth"
        "status"
    )
}

# ─── Fixture payloads ────────────────────────────────────────────────────

issue_search_json() {
    cat <<'EOF'
{"data":{"search":{"nodes":[{"id":"I_kwADOAAAABc","number":265,"title":"Linux keyboard behavior","state":"OPEN","author":{"login":"testuser"},"updatedAt":"2024-01-01T00:00:00Z","assignees":{"nodes":[]},"labels":{"nodes":[]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
EOF
}

issue_view_json() {
    cat <<'EOF'
{"number":265,"title":"Linux keyboard behavior","state":"OPEN","author":{"login":"testuser"},"createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z","labels":{"nodes":[]},"assignees":{"nodes":[]},"milestone":null,"body":"Issue #265 detail body","url":"https://github.com/owner/repo-265/issues/265","id":"I_kwADOAAAABc","comments":[]}
EOF
}

issue_comments_json() {
    cat <<'EOF'
{"data":{"repository":{"issue":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
EOF
}

# ─── Exact-match command routing ─────────────────────────────────────────
#
# The COMPLETE actual argv (all positional args after "gh") is compared
# element-by-element against each allowlisted vector. No flag parsing, no
# substring matching, no prefix variables. Any deviation — reordered args,
# duplicate flags, wrong values, extra args, marker-containing arbitrary
# GraphQL — is rejected.

main() {
    # Capture the complete actual argv as an array.
    local -a actual=("$@")

    [[ ${#actual[@]} -gt 0 ]] || {
        reject "no subcommand" ""
    }

    # Build all expected vectors once.
    build_search_argv
    build_issue_view_argv
    build_comments_argv
    build_auth_status_argv

    # ── Operation 1/4: GraphQL issue search list ──
    if argv_eq SEARCH_ARGV actual; then
        audit_accept "search" "${actual[@]}"
        issue_search_json
        return 0
    fi

    # ── Operation 2: `gh issue view --json` detail read ──
    if argv_eq ISSUE_VIEW_ARGV actual; then
        audit_accept "issue-view" "${actual[@]}"
        issue_view_json
        return 0
    fi

    # ── Operation 3: GraphQL issue comments ──
    if argv_eq COMMENTS_ARGV actual; then
        audit_accept "comments" "${actual[@]}"
        issue_comments_json
        return 0
    fi

    # ── Optional: `gh auth status` exact vector ──
    # check_auth() is not called at startup in the current production code
    # path (it is only invoked when the auth dialog is opened). However, if
    # it IS invoked, only the exact two-word vector is accepted — any
    # trailing or reordered arguments are rejected.
    if argv_eq AUTH_STATUS_ARGV actual; then
        # Auth-status calls are audited as ACCEPTED so the scenario runner
        # can account for them, but they do not count toward the four
        # issue-read operations.
        audit_accept "auth-status" "${actual[@]}"
        echo "github.com"
        echo "  Logged in to github.com account testuser"
        return 0
    fi

    # ── Reject everything else ──
    # This covers all mutations (POST/PATCH/DELETE), issue create/close/
    # delete/comment lifecycle commands, and any argv drift on the four
    # allowlisted vectors.
    reject "unmatched argv (not an exact allowlisted vector)" "${actual[@]}"
}

main "$@"
