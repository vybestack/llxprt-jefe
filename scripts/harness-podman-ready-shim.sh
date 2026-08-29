#!/bin/sh
# Container-runtime readiness fixture for scenarios whose subject is not the
# sandbox host itself. Launch preflight asks the configured engine whether its
# remote socket exists; the harness PATH is hermetic, so without this fixture no
# engine is present and every sandbox-enabled launch is gated by a prompt.
set -eu

case "${1:-}" in
    info)
        printf 'true\n'
        ;;
    *)
        printf 'unsupported harness podman invocation:' >&2
        printf ' %s' "$@" >&2
        printf '\n' >&2
        exit 64
        ;;
esac
