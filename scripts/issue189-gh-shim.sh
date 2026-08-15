#!/bin/bash
# Fail-closed read-only gh shim for issue #189 PR composer scenario.
set -euo pipefail

SHIM_OWNED_AUDIT=false
if [[ -n "${GH_SHIM_AUDIT:-}" ]]; then
    AUDIT_FILE="$GH_SHIM_AUDIT"
else
    AUDIT_FILE=$(mktemp "${TMPDIR:-/tmp}/jefe-issue189-gh-audit.XXXXXX.log") || {
        echo "gh shim: failed to create a private audit file" >&2
        exit 2
    }
    SHIM_OWNED_AUDIT=true
fi
if ! : 2>/dev/null >> "$AUDIT_FILE"; then
    echo "gh shim: audit file is not writable: $AUDIT_FILE" >&2
    exit 2
fi

cleanup_shim_audit() {
    if [[ "$SHIM_OWNED_AUDIT" == true && -f "$AUDIT_FILE" ]]; then
        rm -f "$AUDIT_FILE" 2>/dev/null || true
    fi
}
trap cleanup_shim_audit EXIT

audit_write() {
    printf '%s\n' "$1" >> "$AUDIT_FILE"
}

audit_accept() {
    local op="$1"; shift
    audit_write "ACCEPTED $op -- gh $(printf '%q ' "$@")"
    : > "${AUDIT_FILE}.accepted-${op}"
}

audit_reject() {
    local reason="$1"; shift
    : > "${AUDIT_FILE}.rejected"
    audit_write "REJECTED $reason -- gh $(printf '%q ' "$@")"
}

if [[ "$*" == "auth status" ]]; then
    audit_accept "auth-status" "$@"
    exit 0
fi

if [[ "${1:-}" == "api" && "${2:-}" == "graphql" ]]; then
    query_arg=""
    search_query=""
    for ((i = 3; i <= $#; i++)); do
        if [[ "${!i}" == "-f" ]]; then
            next=$((i + 1))
            if (( next <= $# )); then
                val="${!next}"
                if [[ "$val" == query=* ]]; then
                    query_arg="${val#query=}"
                fi
            fi
        elif [[ "${!i}" == "-F" ]]; then
            next=$((i + 1))
            if (( next <= $# )); then
                val="${!next}"
                if [[ "$val" == searchQuery=* ]]; then
                    search_query="${val#searchQuery=}"
                fi
            fi
        fi
    done

    if [[ "$query_arg" == *"search(type: ISSUE"* && -n "$search_query" ]]; then
        audit_accept "pr-search" "$@"
        cat <<'JSON'
{"data":{"search":{"nodes":[{"number":238,"title":"Add newest-first review sorting","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"updatedAt":"2026-07-03T12:00:00Z","headRefName":"feature-238","headRefOid":"abc238def","baseRefName":"main","isDraft":false,"reviewDecision":null,"statusCheckRollup":{"contexts":{"nodes":[]}},"assignees":{"nodes":[]},"labels":{"nodes":[]},"comments":{"totalCount":0},"body":"Test PR for review ordering"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
JSON
        exit 0
    fi

    if [[ "$query_arg" == *"reviewThreads"* ]]; then
        audit_accept "review-threads" "$@"
        cat <<'JSON'
{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
JSON
        exit 0
    fi

    if [[ "$query_arg" == *"pullRequest(number:"* && "$query_arg" == *"comments(first:"* && "$query_arg" != *"reviewThreads"* ]]; then
        audit_accept "pr-comments" "$@"
        cat <<'JSON'
{"data":{"repository":{"pullRequest":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
JSON
        exit 0
    fi
fi

if [[ "${1:-} ${2:-}" == "pr view" ]]; then
    has_number=0
    has_json=0
    for arg in "${@:3}"; do
        [[ "$arg" == "238" ]] && has_number=1
        [[ "$arg" == "--json" ]] && has_json=1
    done
    if (( has_number && has_json )); then
        audit_accept "pr-view-detail" "$@"
        cat <<'JSON'
{"number":238,"title":"Add newest-first review sorting","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"createdAt":"2026-07-01T00:00:00Z","updatedAt":"2026-07-03T12:00:00Z","headRefName":"feature-238","headRefOid":"abc238def","baseRefName":"main","isDraft":false,"labels":[],"assignees":[],"milestone":null,"body":"Test PR for composer submit behavior","url":"https://github.com/owner/review-sort-fixture/pull/238","reviewDecision":null,"mergeable":null,"mergeStateStatus":null,"reviews":[]}
JSON
        exit 0
    fi
fi

audit_reject "unexpected-argv" "$@"
printf 'gh shim: REJECTED unexpected gh argv: ' >&2
printf '%q ' "$@" >&2
printf '\n' >&2
exit 64
