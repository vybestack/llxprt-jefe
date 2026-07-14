# Shared, data-only fixture constants for the issue #265 gh shim.
#
# This file is NOT executable. It declares the four readonly production
# argument-vector constants exactly once so the shim (issue265-gh-shim.sh)
# and its self-test (issue265-shim-selftest.sh) never drift apart. Source
# it with:
#
#   SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#   . "$SCRIPT_DIR/issue265-gh-shim-fixtures.sh"
#
# No shebang, no set, no executable bit: this is pure data.

if [[ "${_ISSUE265_GH_SHIM_FIXTURES_SOURCED:-0}" == 1 ]]; then
    return 0 2>/dev/null || true
fi
_ISSUE265_GH_SHIM_FIXTURES_SOURCED=1

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
