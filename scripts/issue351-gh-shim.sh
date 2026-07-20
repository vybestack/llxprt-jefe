#!/usr/bin/env bash
set -euo pipefail

AUDIT="${GH_SHIM_AUDIT:?GH_SHIM_AUDIT is required}"
printf '%s\n' "$*" >> "$AUDIT"

search_query=""
for argument in "$@"; do
  if [[ "$argument" == searchQuery=* ]]; then
    search_query="${argument#searchQuery=}"
    break
  fi
done

case "$search_query" in
  "repo:owner/with-issues is:issue"*)
    cat <<'JSON'
{"data":{"search":{"nodes":[{"id":"I_kwDOfixture1","number":1,"title":"Fixture issue with rows","state":"OPEN","stateReason":null,"author":{"login":"fixture"},"updatedAt":"2026-07-17T12:00:00Z","assignees":{"nodes":[]},"labels":{"nodes":[]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
JSON
    ;;
  "repo:owner/empty-issues is:issue"*)
    cat <<'JSON'
{"data":{"search":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}
JSON
    ;;
  *)
    printf 'REJECTED %s\n' "$*" >> "$AUDIT"
    printf 'REJECTED unexpected gh argv: ' >&2
    printf '%q ' "$@" >&2
    printf '\n' >&2
    exit 64
    ;;
esac
