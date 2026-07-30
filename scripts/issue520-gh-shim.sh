#!/bin/sh
set -eu

if [ -d .git ] && ! git rev-parse --verify HEAD >/dev/null 2>&1; then
  git config user.name fixture
  git config user.email fixture@example.com
  git commit --allow-empty --quiet -m fixture
  git update-ref refs/remotes/origin/main HEAD
fi

case "${1:-} ${2:-}" in
  "auth status")
    printf '%s\n' 'github.com' '  Logged in to github.com account testuser'
    ;;
  "issue view")
    printf '%s\n' '{"number":230,"title":"Agent chooser identity and worktree status","state":"OPEN","author":{"login":"testuser"},"createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z","labels":{"nodes":[]},"assignees":{"nodes":[]},"milestone":null,"body":"Issue #230 detail body","url":"https://github.com/owner/repo-230/issues/230","id":"I_kwADOAAAABc230","comments":[]}'
    ;;
  "api graphql")
    case "$*" in
      *"search(type: ISSUE"*)
        printf '%s\n' '{"data":{"search":{"nodes":[{"id":"I_kwADOAAAABc230","number":230,"title":"Agent chooser identity and worktree status","state":"OPEN","author":{"login":"testuser"},"updatedAt":"2024-01-01T00:00:00Z","assignees":{"nodes":[]},"labels":{"nodes":[]},"issueType":null,"milestone":null,"comments":{"totalCount":0}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}'
        ;;
      *"issue(number:"*)
        printf '%s\n' '{"data":{"repository":{"issue":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
        ;;
      *)
        printf 'issue520 gh shim: rejected GraphQL request\n' >&2
        exit 1
        ;;
    esac
    ;;
  *)
    printf 'issue520 gh shim: rejected command\n' >&2
    exit 1
    ;;
esac
