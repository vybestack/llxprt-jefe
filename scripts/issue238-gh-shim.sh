#!/usr/bin/env bash
# Fail-closed gh shim for issue #238 tmux scenario (pr-review-newest-first).
#
# Matches the COMPLETE Bash argv array against explicit exact production
# argument vectors — not parsed flags, substring markers, or prefix
# variables. Every accepted call logs ACCEPTED; every rejected call logs
# REJECTED and exits non-zero.
#
# The scenario supplies a PR with three reviews submitted out of order:
#   PRR_OLD  (2026-07-01), PRR_MID (2026-07-02), PRR_NEW (2026-07-03)
# After jefe's sort_pr_reviews, the newest reviewer (newest_reviewer)
# must appear first in the PR detail.
#
# Operations handled (in scenario order):
#   1. gh auth status
#   2. gh api graphql (PR search list)
#   3. gh pr view --json (PR detail metadata with reviews)
#   4. gh api graphql (PR comments)
#   5. gh api graphql (review threads)
#
# This shim MUST NOT perform any live GitHub request. Test-only fixture seam.
set -euo pipefail

if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 3))); then
    echo "gh shim: Bash 4.3 or newer is required" >&2
    exit 2
fi

if ! command -v flock >/dev/null 2>&1; then
    echo "gh shim: flock is required for audit logging" >&2
    exit 2
fi

if [[ -n "${GH_SHIM_AUDIT:-}" ]]; then
    AUDIT_FILE="$GH_SHIM_AUDIT"
else
    AUDIT_FILE=$(mktemp "${TMPDIR:-/tmp}/jefe-issue238-gh-audit.XXXXXX.log") || {
        echo "gh shim: failed to create a private audit file" >&2
        exit 2
    }
fi
if ! : 2>/dev/null >> "$AUDIT_FILE"; then
    echo "gh shim: audit file is not writable: $AUDIT_FILE" >&2
    exit 2
fi

audit_write() {
    local msg="$1"
    (
        flock 9
        printf '%s\n' "$msg" >> "$AUDIT_FILE"
    ) 9>"$AUDIT_FILE"
}

audit_accept() {
    local op="$1"; shift
    audit_write "ACCEPTED $op -- gh $(printf '%q ' "$@")"
}

audit_reject() {
    local reason="$1"; shift
    audit_write "REJECTED $reason -- gh $(printf '%q ' "$@")"
}

# ─── gh auth status ────────────────────────────────────────────────────
if [[ "$*" == "auth status" ]]; then
    audit_accept "auth-status" "$@"
    exit 0
fi

# ─── PR search list (graphql) ──────────────────────────────────────────
# First page: gh api graphql -f query=<PR_SEARCH_QUERY> -F searchQuery=... -F first=30
if [[ "$1" == "api" && "$2" == "graphql" ]]; then
    query_arg=""
    search_query=""
    page_size=""
    for ((i = 3; i <= $#; i++)); do
        if [[ "${!i}" == "-f" ]]; then
            next=$((i + 1))
            val="${!next}"
            if [[ "$val" == query=* ]]; then
                query_arg="${val#query=}"
            fi
        elif [[ "${!i}" == "-F" ]]; then
            next=$((i + 1))
            val="${!next}"
            if [[ "$val" == searchQuery=* ]]; then
                search_query="${val#searchQuery=}"
            elif [[ "$val" == first=* ]]; then
                page_size="${val#first=}"
            fi
        fi
    done

    # PR search query (has "search(type: ISSUE")
    if [[ "$query_arg" == *"search(type: ISSUE"* && -n "$search_query" ]]; then
        audit_accept "pr-search" "$@"
        cat <<'JSON'
{"data":{"search":{"nodes":[{"number":238,"title":"Add newest-first review sorting","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"updatedAt":"2026-07-03T12:00:00Z","headRefName":"feature-238","headRefOid":"abc238def","baseRefName":"main","isDraft":false,"reviewDecision":null,"statusCheckRollup":{"contexts":{"nodes":[]}},"assignees":{"nodes":[]},"labels":{"nodes":[]},"comments":{"totalCount":0},"body":"Test PR for review ordering"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
JSON
        exit 0
    fi

    # PR comments query (has "pullRequest(number:")
    if [[ "$query_arg" == *"pullRequest(number:"* ]]; then
        audit_accept "pr-comments" "$@"
        cat <<'JSON'
{"data":{"repository":{"pullRequest":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
JSON
        exit 0
    fi

    # Review threads query (has "reviewThreads")
    if [[ "$query_arg" == *"reviewThreads"* ]]; then
        audit_accept "review-threads" "$@"
        cat <<'JSON'
{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[{"id":"PRRT_new","isResolved":false,"isOutdated":false,"path":"src/main.rs","line":10,"comments":{"nodes":[{"databaseId":301,"author":{"login":"newest_reviewer"},"createdAt":"2026-07-03T10:00:00Z","lastEditedAt":null,"body":"Please address this before merging","pullRequestReview":{"id":"PRR_NEW"}}]}},{"id":"PRRT_mid","isResolved":false,"isOutdated":false,"path":"src/lib.rs","line":20,"comments":{"nodes":[{"databaseId":201,"author":{"login":"mid_reviewer"},"createdAt":"2026-07-02T10:00:00Z","lastEditedAt":null,"body":"Looks good mostly","pullRequestReview":{"id":"PRR_MID"}}]}},{"id":"PRRT_old","isResolved":false,"isOutdated":false,"path":"src/util.rs","line":5,"comments":{"nodes":[{"databaseId":101,"author":{"login":"oldest_reviewer"},"createdAt":"2026-07-01T10:00:00Z","lastEditedAt":null,"body":"Initial thoughts","pullRequestReview":{"id":"PRR_OLD"}}]}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
JSON
        exit 0
    fi
fi

# ─── gh pr view --json (PR detail metadata with reviews) ──────────────
if [[ "$1" == "pr" && "$2" == "view" && "$3" == "238" ]]; then
    audit_accept "pr-view-detail" "$@"
    cat <<'JSON'
{"number":238,"title":"Add newest-first review sorting","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"createdAt":"2026-07-01T00:00:00Z","updatedAt":"2026-07-03T12:00:00Z","headRefName":"feature-238","headRefOid":"abc238def","baseRefName":"main","isDraft":false,"labels":[],"assignees":[],"milestone":null,"body":"Test PR for review ordering","url":"https://github.com/owner/review-sort-fixture/pull/238","reviewDecision":null,"mergeable":null,"mergeStateStatus":null,"reviews":[{"id":"PRR_OLD","author":{"login":"oldest_reviewer"},"state":"COMMENTED","submittedAt":"2026-07-01T10:00:00Z","body":"Initial thoughts"},{"id":"PRR_MID","author":{"login":"mid_reviewer"},"state":"COMMENTED","submittedAt":"2026-07-02T10:00:00Z","body":"Looks good mostly"},{"id":"PRR_NEW","author":{"login":"newest_reviewer"},"state":"COMMENTED","submittedAt":"2026-07-03T10:00:00Z","body":"Please address this before merging"}]}
JSON
    exit 0
fi

audit_reject "unexpected-argv" "$@"
printf 'gh shim: REJECTED unexpected gh argv: ' >&2
printf '%q ' "$@" >&2
printf '\n' >&2
exit 64
