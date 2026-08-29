#!/bin/sh
# Loaded-SSH-agent fixture for scenarios whose subject is not the sandbox host
# itself. Launch preflight reads `ssh-add -l` to decide whether the forwarded
# agent holds a key; this fixture reports exactly one, so the launch is not
# gated. Scenarios that mean to observe the empty-agent prompt must not install
# it.
set -eu

case "${1:-}" in
    -l|-L)
        printf '256 SHA256:harnessfixtureidentity harness@fixture (ED25519)\n'
        ;;
    *)
        printf 'unsupported harness ssh-add invocation:' >&2
        printf ' %s' "$@" >&2
        printf '\n' >&2
        exit 64
        ;;
esac
