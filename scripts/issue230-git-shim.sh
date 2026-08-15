#!/bin/sh
set -eu

[ "${1:-}" = "-C" ] || {
    printf 'unexpected git invocation: %s\n' "$*" >&2
    exit 64
}
work_dir=${2:?missing git work directory}
shift 2

case "$work_dir:$*" in
    llx-repo:"rev-parse --abbrev-ref HEAD"|*/llx-repo:"rev-parse --abbrev-ref HEAD")
        printf 'main\n'
        ;;
    llx-repo:"status --porcelain=v1 -z"|*/llx-repo:"status --porcelain=v1 -z")
        printf ' M src-change.txt\0'
        ;;
    pup-repo:"rev-parse --abbrev-ref HEAD"|*/pup-repo:"rev-parse --abbrev-ref HEAD")
        printf 'feature\n'
        ;;
    pup-repo:"status --porcelain=v1 -z"|*/pup-repo:"status --porcelain=v1 -z")
        ;;
    *)
        printf 'unexpected git invocation in %s: %s\n' "$work_dir" "$*" >&2
        exit 64
        ;;
esac
