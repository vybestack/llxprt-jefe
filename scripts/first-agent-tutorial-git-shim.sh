#!/bin/sh
set -eu
case "$*" in
    "rev-parse --is-inside-work-tree")
        printf 'true\n'
        ;;
    "remote get-url origin")
        printf 'https://github.com/vybestack/llxprt-jefe.git\n'
        ;;
    "symbolic-ref refs/remotes/origin/HEAD")
        printf 'refs/remotes/origin/main\n'
        ;;
    "status --porcelain=v1 -z")
        ;;
    "rev-parse --abbrev-ref HEAD")
        printf 'main\n'
        ;;
    "checkout -B main origin/main --"|"reset --hard origin/main"|"fetch origin main")
        ;;
    *)
        printf 'tutorial git fixture rejected: git %s\n' "$*" >&2
        exit 1
        ;;
esac
