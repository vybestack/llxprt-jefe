#!/bin/sh
# Fail-closed local GitHub fixture for the first-agent tutorial capture.
set -eu

: "${TUTORIAL_GH_AUDIT:?TUTORIAL_GH_AUDIT is required}"
: "${TUTORIAL_GH_STATE:?TUTORIAL_GH_STATE is required}"

REPO=vybestack/llxprt-jefe
ISSUE=352
PR=353

ISSUE_SEARCH_QUERY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on Issue { id number title state stateReason author { login } updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } } } pageInfo { hasNextPage endCursor } } }'
ISSUE_COMMENTS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }'
PR_SEARCH_QUERY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on PullRequest { number title state mergedAt author { login } updatedAt headRefName headRefOid baseRefName isDraft mergeable reviewDecision statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun { name status conclusion detailsUrl } ... on StatusContext { context state targetUrl } } } } assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } comments { totalCount } body } } pageInfo { hasNextPage endCursor } } }'
PR_COMMENTS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } totalCount } } } }'
PR_THREADS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { reviewThreads(first: $first) { nodes { id isResolved isOutdated path line comments(first: 50) { nodes { databaseId author { login } createdAt lastEditedAt body pullRequestReview { id } } } } pageInfo { hasNextPage endCursor } } } } }'
ISSUE_VIEW_FIELDS='number,title,state,state_reason,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id'
PR_VIEW_FIELDS='number,title,state,mergedAt,author,createdAt,updatedAt,headRefName,headRefOid,baseRefName,isDraft,labels,assignees,milestone,body,url,reviewDecision,statusCheckRollup,reviews,mergeable,mergeStateStatus'

log() {
    printf '%s\n' "$1" >> "$TUTORIAL_GH_AUDIT"
}

reject() {
    log "REJECTED $*"
    printf 'tutorial gh fixture rejected: gh %s\n' "$*" >&2
    exit 1
}

pr_is_merged() {
    [ "$(cat "$TUTORIAL_GH_STATE" 2>/dev/null || true)" = merged ]
}

issue_search_json() {
    cat <<'EOF'
{"data":{"search":{"nodes":[{"id":"I_TUTORIAL352","number":352,"title":"Turn the getting-started guide into a filled-in visual happy path","state":"OPEN","stateReason":null,"author":{"login":"tutorial-user"},"updatedAt":"2026-07-17T16:22:26Z","assignees":{"nodes":[]},"labels":{"nodes":[{"name":"documentation"},{"name":"enhancement"}]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
EOF
}

issue_view_json() {
    cat <<'EOF'
{"number":352,"title":"Turn the getting-started guide into a filled-in visual happy path","state":"OPEN","state_reason":null,"author":{"login":"tutorial-user"},"createdAt":"2026-07-17T16:22:00Z","updatedAt":"2026-07-17T16:22:26Z","labels":[{"name":"documentation"},{"name":"enhancement"}],"assignees":[],"milestone":null,"body":"Rewrite the getting-started guide as one filled-in visual happy path from repository setup through merge.","url":"https://github.com/vybestack/llxprt-jefe/issues/352","comments":[],"id":"I_TUTORIAL352"}
EOF
}

issue_comments_json() {
    printf '%s\n' '{"data":{"repository":{"issue":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

pr_search_json() {
    if pr_is_merged; then state=MERGED; merged_at='"2026-07-17T17:00:00Z"'; else state=OPEN; merged_at=null; fi
    printf '%s\n' "{\"data\":{\"search\":{\"nodes\":[{\"number\":353,\"title\":\"Complete the filled-in getting-started walkthrough\",\"state\":\"$state\",\"mergedAt\":$merged_at,\"author\":{\"login\":\"tutorial-agent\"},\"updatedAt\":\"2026-07-17T16:45:00Z\",\"headRefName\":\"issue352\",\"headRefOid\":\"3533533533533533533533533533533533533533\",\"baseRefName\":\"main\",\"isDraft\":false,\"mergeable\":\"MERGEABLE\",\"reviewDecision\":\"APPROVED\",\"statusCheckRollup\":{\"contexts\":{\"nodes\":[{\"__typename\":\"CheckRun\",\"name\":\"ci\",\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\",\"detailsUrl\":\"https://github.com/vybestack/llxprt-jefe/actions/runs/353\"}]}},\"assignees\":{\"nodes\":[]},\"labels\":{\"nodes\":[{\"name\":\"documentation\"}]},\"comments\":{\"totalCount\":0},\"body\":\"Implements issue 352 with matching LLxprt Code, Code Puppy, Issues, and Pull Requests screenshots.\"}],\"pageInfo\":{\"hasNextPage\":false,\"endCursor\":null}}}}"
}

pr_view_json() {
    if pr_is_merged; then state=MERGED; merged_at='"2026-07-17T17:00:00Z"'; else state=OPEN; merged_at=null; fi
    printf '%s\n' "{\"number\":353,\"title\":\"Complete the filled-in getting-started walkthrough\",\"state\":\"$state\",\"mergedAt\":$merged_at,\"author\":{\"login\":\"tutorial-agent\"},\"createdAt\":\"2026-07-17T16:40:00Z\",\"updatedAt\":\"2026-07-17T16:45:00Z\",\"headRefName\":\"issue352\",\"headRefOid\":\"3533533533533533533533533533533533533533\",\"baseRefName\":\"main\",\"isDraft\":false,\"labels\":[{\"name\":\"documentation\"}],\"assignees\":[],\"milestone\":null,\"body\":\"Implements issue 352 with matching LLxprt Code, Code Puppy, Issues, and Pull Requests screenshots.\",\"url\":\"https://github.com/vybestack/llxprt-jefe/pull/353\",\"reviewDecision\":\"APPROVED\",\"statusCheckRollup\":[{\"__typename\":\"CheckRun\",\"name\":\"ci\",\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\",\"detailsUrl\":\"https://github.com/vybestack/llxprt-jefe/actions/runs/353\"}],\"reviews\":[{\"id\":\"PRR_TUTORIAL353\",\"author\":{\"login\":\"reviewer\"},\"state\":\"APPROVED\",\"submittedAt\":\"2026-07-17T16:50:00Z\",\"body\":\"The tutorial is clear and consistent.\"}],\"mergeable\":true,\"mergeStateStatus\":\"CLEAN\"}"
}

pr_comments_json() {
    printf '%s\n' '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null},"totalCount":0}}}}}'
}

pr_threads_json() {
    printf '%s\n' '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

if [ "$#" -eq 2 ] && [ "$1" = auth ] && [ "$2" = status ]; then
    log 'ACCEPTED auth-status'
    printf '%s\n' 'github.com' '  Logged in to github.com account tutorial-user'
elif [ "$#" -eq 8 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$ISSUE_SEARCH_QUERY" ] && [ "$5" = -F ] && [ "$6" = "searchQuery=repo:$REPO is:issue state:open" ] && [ "$7" = -F ] && [ "$8" = first=30 ]; then
    log 'ACCEPTED issue-search'
    issue_search_json
elif [ "$#" -eq 7 ] && [ "$1" = issue ] && [ "$2" = view ] && [ "$3" = --repo ] && [ "$4" = "$REPO" ] && [ "$5" = "$ISSUE" ] && [ "$6" = --json ] && [ "$7" = "$ISSUE_VIEW_FIELDS" ]; then
    log 'ACCEPTED issue-view'
    issue_view_json
elif [ "$#" -eq 12 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$ISSUE_COMMENTS_QUERY" ] && [ "$5" = -F ] && [ "$6" = owner=vybestack ] && [ "$7" = -F ] && [ "$8" = repo=llxprt-jefe ] && [ "$9" = -F ] && [ "${10}" = number=352 ] && [ "${11}" = -F ] && [ "${12}" = first=30 ]; then
    log 'ACCEPTED issue-comments'
    issue_comments_json
elif [ "$#" -eq 4 ] && [ "$1" = api ] && [ "$2" = user ] && [ "$3" = --jq ] && [ "$4" = .login ]; then
    log 'ACCEPTED viewer-login'
    printf '%s\n' tutorial-user
elif [ "$#" -eq 6 ] && [ "$1" = api ] && [ "$2" = --method ] && [ "$3" = POST ] && [ "$4" = /repos/vybestack/llxprt-jefe/issues/352/assignees ] && [ "$5" = -f ] && [ "$6" = 'assignees[]=tutorial-user' ]; then
    log 'ACCEPTED issue-assign'
    printf '%s\n' '{"assignees":[{"login":"tutorial-user"}]}'
elif [ "$#" -eq 8 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$PR_SEARCH_QUERY" ] && [ "$5" = -F ] && [ "$6" = "searchQuery=repo:$REPO is:pr is:open" ] && [ "$7" = -F ] && [ "$8" = first=30 ]; then
    log 'ACCEPTED pr-search'
    pr_search_json
elif [ "$#" -eq 7 ] && [ "$1" = pr ] && [ "$2" = view ] && [ "$3" = "$PR" ] && [ "$4" = --repo ] && [ "$5" = "$REPO" ] && [ "$6" = --json ] && [ "$7" = "$PR_VIEW_FIELDS" ]; then
    log 'ACCEPTED pr-view'
    pr_view_json
elif [ "$#" -eq 12 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$PR_COMMENTS_QUERY" ] && [ "$5" = -F ] && [ "$6" = owner=vybestack ] && [ "$7" = -F ] && [ "$8" = repo=llxprt-jefe ] && [ "$9" = -F ] && [ "${10}" = number=353 ] && [ "${11}" = -F ] && [ "${12}" = first=30 ]; then
    log 'ACCEPTED pr-comments'
    pr_comments_json
elif [ "$#" -eq 12 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$PR_THREADS_QUERY" ] && [ "$5" = -F ] && [ "$6" = owner=vybestack ] && [ "$7" = -F ] && [ "$8" = repo=llxprt-jefe ] && [ "$9" = -F ] && [ "${10}" = number=353 ] && [ "${11}" = -F ] && [ "${12}" = first=100 ]; then
    log 'ACCEPTED pr-threads'
    pr_threads_json
elif [ "$#" -eq 4 ] && [ "$1" = api ] && [ "$2" = repos/vybestack/llxprt-jefe ] && [ "$3" = --jq ] && [ "$4" = '{allow_merge_commit, allow_squash_merge, allow_rebase_merge}' ]; then
    log 'ACCEPTED merge-methods'
    printf '%s\n' '{"allow_merge_commit":true,"allow_squash_merge":true,"allow_rebase_merge":true}'
elif [ "$#" -eq 6 ] && [ "$1" = pr ] && [ "$2" = merge ] && [ "$3" = "$PR" ] && [ "$4" = --repo ] && [ "$5" = "$REPO" ] && [ "$6" = --squash ]; then
    log 'ACCEPTED pr-merge'
    printf '%s\n' merged > "$TUTORIAL_GH_STATE"
else
    reject "$@"
fi
