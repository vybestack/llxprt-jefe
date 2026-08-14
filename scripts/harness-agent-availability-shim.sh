#!/bin/sh
# Executable-presence fixture for scenarios that must discover an agent type
# without launching a real external agent.
set -eu

case "${1:-}" in
    --version)
        printf '0.0.0-harness\n'
        ;;
    --help)
        printf 'Usage: code-puppy [OPTIONS]\n'
        ;;
    *)
        printf 'unsupported harness agent invocation:' >&2
        printf ' %s' "$@" >&2
        printf '\n' >&2
        exit 64
        ;;
esac
