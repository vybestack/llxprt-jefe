#!/bin/sh
# Fail-closed GitHub fixture for schema-1 issue, PR, and Actions scenarios.
set -eu

: "${HARNESS_GH_AUDIT:?HARNESS_GH_AUDIT is required}"
: "${HARNESS_GH_STATE:?HARNESS_GH_STATE is required}"

REPO=vybestack/llxprt-jefe
ISSUE=352
PR=353

ISSUE_SEARCH_QUERY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on Issue { id number title state stateReason author { login } createdAt updatedAt assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } issueType { name } milestone { title } comments { totalCount } issueFieldValues(first: 10) { nodes { __typename ... on IssueFieldSingleSelectValue { name field { ... on IssueFieldSingleSelect { name } } } } } timelineItems(first: 15, itemTypes: [CROSS_REFERENCED_EVENT]) { nodes { ... on CrossReferencedEvent { source { ... on PullRequest { number } } } } } } } pageInfo { hasNextPage endCursor } } }'
ISSUE_COMMENTS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { issue(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } } } } }'
PR_SEARCH_QUERY='query($searchQuery: String!, $first: Int!) { search(type: ISSUE, query: $searchQuery, first: $first) { nodes { ... on PullRequest { number title state mergedAt author { login } createdAt updatedAt headRefName headRefOid baseRefName isDraft mergeable reviewDecision statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun { name status conclusion detailsUrl startedAt completedAt checkSuite { app { slug } workflowRun { workflow { name } } } } ... on StatusContext { context state targetUrl } } } } assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name } } comments { totalCount } body } } pageInfo { hasNextPage endCursor } } }'
PR_COMMENTS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { comments(first: $first) { nodes { id databaseId author { login } createdAt lastEditedAt body } pageInfo { hasNextPage endCursor } totalCount } } } }'
PR_THREADS_QUERY='query($owner: String!, $repo: String!, $number: Int!, $first: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { reviewThreads(first: $first) { nodes { id isResolved isOutdated path line diffSide startDiffSide startLine originalLine originalStartLine comments(first: 50) { nodes { databaseId author { login } createdAt lastEditedAt body pullRequestReview { id } } } } pageInfo { hasNextPage endCursor } } } } }'
ISSUE_VIEW_FIELDS='number,title,state,stateReason,author,createdAt,updatedAt,labels,assignees,milestone,body,url,comments,id'
PR_VIEW_FIELDS='number,title,state,mergedAt,author,createdAt,updatedAt,headRefName,headRefOid,baseRefName,isDraft,labels,assignees,milestone,body,url,reviewDecision,statusCheckRollup,reviews,mergeable,mergeStateStatus'
LABELS_QUERY='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { labels(first: 100, orderBy: {field: NAME, direction: ASC}) { nodes { name } pageInfo { hasNextPage endCursor } } } }'

log() {
    printf '%s\n' "$1" >> "$HARNESS_GH_AUDIT"
}

mark() {
    : > "${HARNESS_GH_AUDIT}.$1"
}

reject() {
    log "REJECTED $*"
    printf 'rejected\n' > "${HARNESS_GH_AUDIT}.rejected"
    printf 'harness GitHub fixture rejected: gh %s\n' "$*" >&2
    exit 1
}

pr_is_merged() {
    [ "$(cat "$HARNESS_GH_STATE" 2>/dev/null || true)" = merged ]
}

issue_search_json() {
    cat <<'EOF'
{"data":{"search":{"nodes":[{"id":"I_TUTORIAL352","number":352,"title":"Contextual allocation fixture","state":"OPEN","stateReason":null,"author":{"login":"tutorial-user"},"updatedAt":"2026-07-17T16:22:26Z","assignees":{"nodes":[]},"labels":{"nodes":[{"name":"documentation"},{"name":"enhancement"}]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
EOF
}

issue_view_json() {
    cat <<'EOF'
{"number":352,"title":"Contextual allocation fixture","state":"OPEN","stateReason":null,"author":{"login":"tutorial-user"},"createdAt":"2026-07-17T16:22:00Z","updatedAt":"2026-07-17T16:22:26Z","labels":[{"name":"documentation"},{"name":"enhancement"}],"assignees":[],"milestone":null,"body":"wrapped-context-head demonstrates contextual allocation across narrow detail panes with enough words to wrap safely before wrapped-context-tail","url":"https://github.com/vybestack/llxprt-jefe/issues/352","comments":[],"id":"I_TUTORIAL352"}
EOF
}

issue_comments_json() {
    printf '%s\n' '{"data":{"repository":{"issue":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

pr_search_json() {
    if pr_is_merged; then state=MERGED; merged_at='"2026-07-17T17:00:00Z"'; else state=OPEN; merged_at=null; fi
    printf '%s\n' "{\"data\":{\"search\":{\"nodes\":[{\"number\":353,\"title\":\"Fixture pull request\",\"state\":\"$state\",\"mergedAt\":$merged_at,\"author\":{\"login\":\"tutorial-agent\"},\"updatedAt\":\"2026-07-17T16:45:00Z\",\"headRefName\":\"issue352\",\"headRefOid\":\"3533533533533533533533533533533533533533\",\"baseRefName\":\"main\",\"isDraft\":false,\"mergeable\":\"MERGEABLE\",\"reviewDecision\":\"APPROVED\",\"statusCheckRollup\":{\"contexts\":{\"nodes\":[{\"__typename\":\"CheckRun\",\"name\":\"ci\",\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\",\"detailsUrl\":\"https://github.com/vybestack/llxprt-jefe/actions/runs/353\"}]}},\"assignees\":{\"nodes\":[]},\"labels\":{\"nodes\":[{\"name\":\"documentation\"}]},\"comments\":{\"totalCount\":0},\"body\":\"Fixture pull request body.\"}],\"pageInfo\":{\"hasNextPage\":false,\"endCursor\":null}}}}"
}

pr_view_json() {
    if pr_is_merged; then state=MERGED; merged_at='"2026-07-17T17:00:00Z"'; else state=OPEN; merged_at=null; fi
    printf '%s\n' "{\"number\":353,\"title\":\"Fixture pull request\",\"state\":\"$state\",\"mergedAt\":$merged_at,\"author\":{\"login\":\"tutorial-agent\"},\"createdAt\":\"2026-07-17T16:40:00Z\",\"updatedAt\":\"2026-07-17T16:45:00Z\",\"headRefName\":\"issue352\",\"headRefOid\":\"3533533533533533533533533533533533533533\",\"baseRefName\":\"main\",\"isDraft\":false,\"labels\":[{\"name\":\"documentation\"}],\"assignees\":[],\"milestone\":null,\"body\":\"Fixture pull request body.\",\"url\":\"https://github.com/vybestack/llxprt-jefe/pull/353\",\"reviewDecision\":\"APPROVED\",\"statusCheckRollup\":[{\"__typename\":\"CheckRun\",\"name\":\"ci\",\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\",\"detailsUrl\":\"https://github.com/vybestack/llxprt-jefe/actions/runs/353\"}],\"reviews\":[{\"id\":\"PRR_TUTORIAL353\",\"author\":{\"login\":\"reviewer\"},\"state\":\"APPROVED\",\"submittedAt\":\"2026-07-17T16:50:00Z\",\"body\":\"The tutorial is clear and consistent.\"}],\"mergeable\":true,\"mergeStateStatus\":\"CLEAN\"}"
}

pr_comments_json() {
    printf '%s\n' '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null},"totalCount":0}}}}}'
}

pr_threads_json() {
    printf '%s\n' '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
}

if [ "$#" -eq 2 ] && [ "$1" = auth ] && [ "$2" = status ]; then
    log 'ACCEPTED auth-status'
    if [ "${HARNESS_GH_MODE:-authenticated}" = unauthenticated ]; then exit 1; fi
    printf '%s\n' 'github.com' '  Logged in to github.com account fixture-user'
elif [ "${HARNESS_GH_MODE:-authenticated}" = unauthenticated ] && [ "$1" = api ] && [ "$2" = graphql ]; then
    log 'ACCEPTED unauthenticated-request'
    printf '%s\n' 'gh is not authenticated. Run: gh auth login' >&2
    exit 1
elif [ "${HARNESS_GH_MODE:-authenticated}" = unauthenticated ] && [ "$#" -eq 13 ] && [ "$1" = auth ] && [ "$2" = login ] && [ "$3" = --hostname ] && [ "$4" = github.com ] && [ "$5" = --git-protocol ] && [ "$6" = https ] && [ "$7" = --web ] && [ "$8" = --scopes ] && [ "$9" = repo ] && [ "${10}" = --scopes ] && [ "${11}" = read:org ] && [ "${12}" = --scopes ] && [ "${13}" = gist ]; then
    log 'ACCEPTED auth-login'
    printf '%s\n' 'authentication cancelled' >&2
    exit 1
elif [ "$#" -eq 10 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -H ] && [ "$4" = 'GraphQL-Features: issue_fields' ] && [ "$5" = -f ] && [ "$6" = "query=$ISSUE_SEARCH_QUERY" ] && [ "$7" = -F ] && [ "$8" = "searchQuery=repo:$REPO is:issue state:open sort:updated-desc" ] && [ "$9" = -F ] && [ "${10}" = first=30 ]; then
    log 'ACCEPTED issue-search'
    mark issue-search
    issue_search_json
elif [ "$#" -eq 7 ] && [ "$1" = issue ] && [ "$2" = view ] && [ "$3" = --repo ] && [ "$4" = "$REPO" ] && [ "$5" = "$ISSUE" ] && [ "$6" = --json ] && [ "$7" = "$ISSUE_VIEW_FIELDS" ]; then
    log 'ACCEPTED issue-view'
    issue_view_json
elif [ "$#" -eq 12 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$ISSUE_COMMENTS_QUERY" ] && [ "$5" = -F ] && [ "$6" = owner=vybestack ] && [ "$7" = -F ] && [ "$8" = repo=llxprt-jefe ] && [ "$9" = -F ] && [ "${10}" = number=352 ] && [ "${11}" = -F ] && [ "${12}" = first=30 ]; then
    log 'ACCEPTED issue-comments'
    issue_comments_json
elif [ "$#" -eq 8 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$LABELS_QUERY" ] && [ "$5" = -F ] && [ "$6" = owner=vybestack ] && [ "$7" = -F ] && [ "$8" = name=llxprt-jefe ]; then
    log 'ACCEPTED labels-list'
    mark labels-list
    printf '%s\n' '{"data":{"repository":{"labels":{"nodes":[{"name":"documentation"},{"name":"enhancement"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}'
elif [ "$#" -eq 4 ] && [ "$1" = api ] && [ "$2" = user ] && [ "$3" = --jq ] && [ "$4" = .login ]; then
    log 'ACCEPTED viewer-login'
    printf '%s\n' tutorial-user
elif [ "$#" -eq 6 ] && [ "$1" = api ] && [ "$2" = --method ] && [ "$3" = POST ] && [ "$4" = /repos/vybestack/llxprt-jefe/issues/352/assignees ] && [ "$5" = -f ] && [ "$6" = 'assignees[]=tutorial-user' ]; then
    log 'ACCEPTED issue-assign'
    printf '%s\n' '{"assignees":[{"login":"tutorial-user"}]}'
elif [ "$#" -eq 8 ] && [ "$1" = api ] && [ "$2" = graphql ] && [ "$3" = -f ] && [ "$4" = "query=$PR_SEARCH_QUERY" ] && [ "$5" = -F ] && [ "$6" = "searchQuery=repo:$REPO is:pr is:open" ] && [ "$7" = -F ] && [ "$8" = first=30 ]; then
    log 'ACCEPTED pr-search'
    mark pr-search
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
elif [ "$#" -eq 2 ] && [ "$1" = api ] && { [ "$2" = 'repos/vybestack/llxprt-jefe/actions/runs?page=1&per_page=30' ] || [ "$2" = 'repos/vybestack/llxprt-jefe/actions/runs?page=1&per_page=30&event=pull_request' ] || [ "$2" = 'repos/vybestack/llxprt-jefe/actions/runs?page=1&per_page=30&event=pull_request&head_sha=3533533533533533533533533533533533533533' ]; }; then
    log 'ACCEPTED actions-runs'
    mark actions-runs
    printf '%s\n' '{"total_count":1,"workflow_runs":[{"id":9001,"name":"workflow","display_title":"Fixture workflow","status":"completed","conclusion":"success","event":"pull_request","head_branch":"fixture","head_sha":"3533533533533533533533533533533533533533","run_number":1,"run_attempt":1,"created_at":"2026-07-17T16:45:00Z","updated_at":"2026-07-17T16:46:00Z","html_url":"https://github.com/vybestack/llxprt-jefe/actions/runs/9001"}]}'
elif [ "$#" -eq 4 ] && [ "$1" = api ] && [ "$2" = repos/vybestack/llxprt-jefe/actions/workflows ] && [ "$3" = --jq ] && [ "$4" = .workflows ]; then
    log 'ACCEPTED actions-workflows'
    mark actions-workflows
    printf '%s\n' '[{"id":7001,"name":"workflow","path":".github/workflows/ci.yml","state":"active"}]'
elif [ "$#" -eq 7 ] && [ "$1" = run ] && [ "$2" = view ] && [ "$3" = --repo ] && [ "$4" = "$REPO" ] && [ "$5" = 9001 ] && [ "$6" = --json ] && [ "$7" = attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,headSha,name,number,startedAt,status,updatedAt,url,workflowDatabaseId,workflowName ]; then
    log 'ACCEPTED actions-run-detail'
    mark actions-run-detail
    printf '%s\n' '{"attempt":1,"conclusion":"success","createdAt":"2026-07-17T16:45:00Z","databaseId":9001,"displayTitle":"Fixture workflow","event":"pull_request","headBranch":"fixture","headSha":"3533533533533533533533533533533533533533","name":"workflow","number":1,"startedAt":"2026-07-17T16:45:01Z","status":"completed","updatedAt":"2026-07-17T16:46:00Z","url":"https://github.com/vybestack/llxprt-jefe/actions/runs/9001","workflowDatabaseId":7001,"workflowName":"workflow"}'
elif [ "$#" -eq 9 ] && [ "$1" = run ] && [ "$2" = view ] && [ "$3" = --repo ] && [ "$4" = "$REPO" ] && [ "$5" = 9001 ] && [ "$6" = --json ] && [ "$7" = jobs ] && [ "$8" = --jq ] && [ "$9" = .jobs ]; then
    log 'ACCEPTED actions-run-jobs'
    mark actions-run-jobs
    printf '%s\n' '[{"databaseId":9101,"name":"test","status":"completed","conclusion":"success","steps":[{"name":"Run tests","status":"completed","conclusion":"success","number":1}]}]'
elif [ "$#" -eq 4 ] && [ "$1" = api ] && [ "$2" = repos/vybestack/llxprt-jefe ] && [ "$3" = --jq ] && [ "$4" = '{allow_merge_commit, allow_squash_merge, allow_rebase_merge}' ]; then
    log 'ACCEPTED merge-methods'
    mark merge-methods
    printf '%s\n' '{"allow_merge_commit":true,"allow_squash_merge":true,"allow_rebase_merge":true}'
elif [ "$#" -eq 6 ] && [ "$1" = pr ] && [ "$2" = merge ] && [ "$3" = "$PR" ] && [ "$4" = --repo ] && [ "$5" = "$REPO" ] && [ "$6" = --squash ]; then
    log 'ACCEPTED pr-merge'
    printf '%s\n' merged > "$HARNESS_GH_STATE"
else
    reject "$@"
fi
