#!/bin/sh
set -eu

name=${0##*/}
mode=${HARNESS_ISSUE382_PROBE_MODE:-standard}

if [ "$mode" = negative ]; then
    case "$name" in
        codex)
            printf '\377\376\377\377\n'
            exit 1
            ;;
        llxprt)
            printf '{"identity":"0.10"}GARBAGE\n'
            exit 0
            ;;
        *)
            printf 'unexpected negative-probe executable: %s\n' "$name" >&2
            exit 64
            ;;
    esac
fi

if [ "$mode" = resolver ]; then
    if [ "$name" = claude ]; then
        printf 'path-claude\n'
        exit 0
    fi
    printf 'unexpected resolver executable: %s\n' "$name" >&2
    exit 64
fi

if [ "$mode" = status-cartesian ]; then
    case "$name" in
        llxprt) printf 'code-puppy 0.0.634\n' ;;
        codex) printf 'codex-cli 0.142.0\n' ;;
        code-puppy) printf 'llxprt 0.10.0\n' ;;
        *)
            printf 'unexpected status-cartesian executable: %s\n' "$name" >&2
            exit 64
            ;;
    esac
    exit 0
fi

case "$name" in
    llxprt)
        printf '0.10.0-nightly.260720.d69bda66a\n'
        ;;
    codex)
        printf 'codex-cli 0.142.0\n'
        ;;
    code-puppy)
        printf '0.0.634\n'
        ;;
    claude)
        printf '2.1.212 (Claude Code)\n'
        ;;
    podman)
        exit 127
        ;;
    *)
        printf 'unexpected issue382 fixture executable: %s\n' "$name" >&2
        exit 64
        ;;
esac
