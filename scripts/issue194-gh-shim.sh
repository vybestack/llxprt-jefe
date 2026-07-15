#!/usr/bin/env bash
set -euo pipefail

AUDIT="${GH_SHIM_AUDIT:?GH_SHIM_AUDIT is required}"
printf '%s\n' "$*" >> "$AUDIT"

case "$*" in
  "api repos/owner/actions-fixture/actions/runs"\?"page=1&per_page=30")
    cat <<'JSON'
{"total_count":1,"workflow_runs":[{"id":19401,"name":"CI","display_title":"Inspectable Actions fixture","head_branch":"issue194","head_sha":"abc194","run_number":194,"event":"push","status":"completed","conclusion":"failure","created_at":"2026-07-14T00:00:00Z","updated_at":"2026-07-14T00:01:00Z"}]}
JSON
    ;;
  "api repos/owner/actions-fixture/actions/workflows --jq .workflows")
    cat <<'JSON'
[{"id":194,"name":"CI","path":".github/workflows/ci.yml","state":"active"}]
JSON
    ;;
  "run view --repo owner/actions-fixture 19401 --json attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,headSha,name,number,startedAt,status,updatedAt,url,workflowDatabaseId,workflowName")
    cat <<'JSON'
{"databaseId":19401,"name":"Inspectable Actions fixture","headBranch":"issue194","headSha":"abc194","number":194,"event":"push","status":"completed","conclusion":"failure","workflowName":"CI","createdAt":"2026-07-14T00:00:00Z","updatedAt":"2026-07-14T00:01:00Z"}
JSON
    ;;
  "run view --repo owner/actions-fixture 19401 --json jobs --jq .jobs")
    cat <<'JSON'
[{"databaseId":19411,"name":"build-linux-with-a-long-job-name","status":"completed","conclusion":"success","steps":[{"name":"Checkout fixture source","status":"completed","conclusion":"success","number":1},{"name":"Compile fixture application","status":"completed","conclusion":"success","number":2}]},{"databaseId":19412,"name":"test-suite-with-a-long-job-name","status":"completed","conclusion":"failure","steps":[{"name":"Run failing fixture tests","status":"completed","conclusion":"failure","number":1}]},{"databaseId":19413,"name":"publish-artifacts","status":"completed","conclusion":"skipped","steps":[]}]
JSON
    ;;
  *)
    printf 'REJECTED %s
' "$*" >> "$AUDIT"
    printf 'REJECTED unexpected gh argv: ' >&2
    printf '%q ' "$@" >&2
    printf '\n' >&2
    exit 64
    ;;
esac
