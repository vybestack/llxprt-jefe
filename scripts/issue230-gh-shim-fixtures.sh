# Shared, data-only fixture constants for the issue #230 gh shim.
#
# This file is NOT executable. It declares the exact production
# argument-vector constants (query bodies, json fields, repo slug, issue
# number) so the shim (issue230-gh-shim.sh) and its self-test
# (issue230-shim-selftest.sh) never drift apart. Source it with:
#
#   SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#   . "$SCRIPT_DIR/issue230-gh-shim-fixtures.sh"
#
# No shebang, no set, no executable bit: this is pure data.
#
# These constants mirror the issue265 fixtures exactly except for the repo
# slug and issue number, which are scenario-specific.

if [[ "${_ISSUE230_GH_SHIM_FIXTURES_SOURCED:-0}" == 1 ]]; then
    return 0 2>/dev/null || true
fi
_ISSUE230_GH_SHIM_FIXTURES_SOURCED=1

# The scenario repository owner/name pair.
readonly ISSUE230_REPO_OWNER="owner"
readonly ISSUE230_REPO_NAME="repo-230"
readonly ISSUE230_REPO_SLUG="owner/repo-230"

# The deterministic issue number for the scenario.
readonly ISSUE230_NUMBER="230"

# The exact GraphQL search-list query body (no cursor — first page).
# Source: src/github/parse.rs build_issue_search_args, cursor=None branch.
readonly ISSUE230_SEARCH_QUERY_BODY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on Issue { id number title state author { login } updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } } } pageInfo { hasNextPage endCursor } } }'

# The exact search query string for the default open filter.
readonly ISSUE230_SEARCH_QUERY_STRING="repo:${ISSUE230_REPO_SLUG} is:issue state:open"

# The exact --json field list for `gh issue view`.
# Source: src/github/mod.rs get_issue_detail.
readonly ISSUE230_VIEW_JSON_FIELDS='number,title,state,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id'

# The exact GraphQL comments query body (no cursor — first page).
# Source: src/github/mod.rs list_comments, cursor=None branch.
readonly ISSUE230_COMMENTS_QUERY_BODY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }'
