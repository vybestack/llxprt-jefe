#!/bin/sh
# Empty-SSH-agent fixture (issue #713). Reproduces the host state that made
# sandboxed agents fail every git operation over SSH: the agent socket is
# forwarded and answering, and it holds nothing. `ssh-add -l` exits zero and
# says so, which is what launch preflight must recognize.
set -eu

case "${1:-}" in
    -l|-L)
        printf 'The agent has no identities.\n'
        ;;
    *)
        printf 'unsupported harness ssh-add invocation:' >&2
        printf ' %s' "$@" >&2
        printf '\n' >&2
        exit 64
        ;;
esac
