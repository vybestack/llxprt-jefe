#!/bin/bash
set -euo pipefail

AUDIT_FILE="${JEFE_GH_AUDIT:?JEFE_GH_AUDIT is required}"

record() {
  local disposition="$1"
  local operation="$2"
  printf '%s %s\n' "$disposition" "$operation" >>"$AUDIT_FILE"
}

accept() {
  local operation="$1"
  shift
  if [[ "$operation" == "auth-status" ]]; then
    : > "${AUDIT_FILE}.auth-status"
    return
  fi
  : > "${AUDIT_FILE}.accepted-${operation}"
  record ACCEPTED "$operation"
}

reject() {
  : > "${AUDIT_FILE}.rejected"
  record REJECTED unexpected "$@"
  printf 'issue621 gh shim: rejected unexpected argv\n' >&2
  exit 64
}

is_query() {
  [[ $# -gt 0 ]] || return 1
  [[ "$1" == query=* && "$1" != *mutation* ]]
}

is_issue_search() {
  [[ $# -eq 10 && "$1" == api && "$2" == graphql && "$3" == -H &&
    "$4" == 'GraphQL-Features: issue_fields' && "$5" == -f ]] || return 1
  is_query "$6" && [[ "$6" == *'... on Issue'* && "$7" == -F &&
    "$8" == 'searchQuery=repo:owner/list-send-fixture is:issue state:open sort:updated-desc' &&
    "$9" == -F && "${10}" == first=30 ]]
}

is_pr_search() {
  [[ $# -eq 8 && "$1" == api && "$2" == graphql && "$3" == -f ]] || return 1
  is_query "$4" && [[ "$4" == *'... on PullRequest'* && "$5" == -F &&
    "$6" == 'searchQuery=repo:owner/list-send-fixture is:pr is:open' &&
    "$7" == -F && "$8" == first=30 ]]
}

is_detail_query() {
  local resource="$1"
  local first="$2"
  shift 2
  [[ $# -eq 12 && "$1" == api && "$2" == graphql && "$3" == -f ]] || return 1
  is_query "$4" && [[ "$4" == *"$resource"* && "$5" == -F && "$6" == owner=owner &&
    "$7" == -F && "$8" == repo=list-send-fixture && "$9" == -F &&
    "${10}" == number=621 && "${11}" == -F && "${12}" == "first=$first" ]]
}

emit_issue_search() {
  printf '%s\n' '{"data":{"search":{"nodes":[{"id":"I_621","number":621,"title":"Send selected issues directly from the list","state":"OPEN","stateReason":null,"author":{"login":"fixture-author"},"createdAt":"2026-08-01T00:00:00Z","updatedAt":"2026-08-02T00:00:00Z","assignees":{"nodes":[]},"labels":{"nodes":[]},"issueType":null,"milestone":null,"comments":{"totalCount":1},"issueFieldValues":{"nodes":[]},"timelineItems":{"nodes":[]}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}'
}

emit_pr_search() {
  printf '%s\n' '{"data":{"search":{"nodes":[{"number":621,"title":"Send selected pull requests directly from the list","state":"OPEN","mergedAt":null,"author":{"login":"fixture-author"},"createdAt":"2026-08-01T00:00:00Z","updatedAt":"2026-08-02T00:00:00Z","headRefName":"issue621","headRefOid":"abc621","baseRefName":"main","isDraft":false,"mergeable":"MERGEABLE","reviewDecision":null,"statusCheckRollup":{"contexts":{"nodes":[]}},"assignees":{"nodes":[]},"labels":{"nodes":[]},"comments":{"totalCount":1},"body":"PR list body"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}'
}

emit_issue_view() {
  printf '%s\n' '{"number":621,"title":"Send selected issues directly from the list","state":"OPEN","stateReason":null,"author":{"login":"fixture-author"},"createdAt":"2026-08-01T00:00:00Z","updatedAt":"2026-08-02T00:00:00Z","labels":[],"assignees":[],"milestone":null,"body":"Complete issue body loaded before chooser","url":"https://github.com/owner/list-send-fixture/issues/621","comments":[],"id":"I_621"}'
}

emit_pr_view() {
  printf '%s\n' '{"number":621,"title":"Send selected pull requests directly from the list","state":"OPEN","mergedAt":null,"isDraft":false,"author":{"login":"fixture-author"},"createdAt":"2026-08-01T00:00:00Z","updatedAt":"2026-08-02T00:00:00Z","headRefName":"issue621","headRefOid":"abc621","baseRefName":"main","labels":[],"assignees":[],"milestone":null,"body":"Complete pull request body loaded before chooser","url":"https://github.com/owner/list-send-fixture/pull/621","comments":[],"reviews":[],"statusCheckRollup":{"contexts":{"nodes":[]}},"mergeable":"MERGEABLE","reviewDecision":null}'
}

emit_issue_comments() {
  printf '%s\n' '{"data":{"repository":{"issue":{"comments":{"nodes":[{"id":"IC_621","databaseId":6211,"author":{"login":"commenter"},"createdAt":"2026-08-02T00:00:00Z","lastEditedAt":null,"body":"Complete issue comment"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

emit_pr_comments() {
  printf '%s\n' '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[{"id":"PC_621","databaseId":6212,"author":{"login":"commenter"},"createdAt":"2026-08-02T00:00:00Z","lastEditedAt":null,"body":"Complete pull request comment"}],"pageInfo":{"hasNextPage":false,"endCursor":null},"totalCount":1}}}}}'
}

emit_review_threads() {
  printf '%s\n' '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

if [[ $# -eq 2 && "$1" == auth && "$2" == status ]]; then
  accept auth-status "$@"
  printf '%s\n' 'Logged in to github.com account fixture-user'
elif [[ $# -eq 7 && "$1" == issue && "$2" == view && "$3" == --repo &&
  "$4" == owner/list-send-fixture && "$5" == 621 && "$6" == --json &&
  "$7" == 'number,title,state,stateReason,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id' ]]; then
  accept issue-view-detail "$@"
  emit_issue_view
elif [[ $# -eq 7 && "$1" == pr && "$2" == view && "$3" == 621 &&
  "$4" == --repo && "$5" == owner/list-send-fixture && "$6" == --json &&
  "$7" == 'number,title,state,mergedAt,author,createdAt,updatedAt,headRefName,headRefOid,baseRefName,isDraft,labels,assignees,milestone,body,url,reviewDecision,statusCheckRollup,reviews,mergeable,mergeStateStatus' ]]; then
  accept pr-view-detail "$@"
  emit_pr_view
elif is_issue_search "$@"; then
  accept issue-search "$@"
  emit_issue_search
elif is_pr_search "$@"; then
  accept pr-search "$@"
  emit_pr_search
elif is_detail_query 'issue(number:' 30 "$@"; then
  accept issue-comments "$@"
  emit_issue_comments
elif is_detail_query 'pullRequest(number:' 30 "$@" && [[ "$4" == *'comments(first:'* && "$4" != *'reviewThreads'* ]]; then
  accept pr-comments "$@"
  emit_pr_comments
elif is_detail_query 'reviewThreads(first:' 100 "$@"; then
  accept pr-review-threads "$@"
  emit_review_threads
else
  reject "$@"
fi
