#!/bin/sh
set -eu

mode=${HARNESS_GIT_MODE:?HARNESS_GIT_MODE is required}
state=${HARNESS_GIT_STATE:?HARNESS_GIT_STATE is required}
audit=${HARNESS_GIT_AUDIT:?HARNESS_GIT_AUDIT is required}

mark() {
    : >"$audit.$1"
}

reject() {
    : >"$audit.rejected"
    printf 'unexpected git invocation in %s (%s):' "$work_dir" "$mode" >&2
    printf ' <%s>' "$@" >&2
    printf '\n' >&2
    exit 64
}

if [ "${1:-}" = "-C" ]; then
    [ "$#" -ge 3 ] || exit 64
    work_dir=$2
    shift 2
else
    work_dir=$PWD
fi

case "$*" in
    "rev-parse --is-inside-work-tree")
        mark inside-worktree
        printf '%s\n' true
        ;;
    "rev-parse --abbrev-ref HEAD")
        printf '%s\n' main
        ;;
    "symbolic-ref refs/remotes/origin/HEAD")
        printf '%s\n' refs/remotes/origin/main
        ;;
    "remote get-url origin")
        mark origin
        if [ "$mode" = origin-mismatch ]; then
            printf '%s\n' https://github.com/other/repository.git
        else
            printf '%s\n' https://github.com/vybestack/llxprt-jefe.git
        fi
        ;;
    "status --porcelain=v1 -z")
        if [ "$mode" = dirty-copy ] && [ "$(/bin/cat "$state")" = dirty ]; then
            mark dirty
            printf ' M README.md\0'
        else
            mark clean
        fi
        ;;
    "fetch origin main" | "checkout -B main origin/main --" | "reset --hard origin/main")
        [ "$mode" = dirty-copy ]
        ;;
    *)
        if [ "$#" -eq 3 ] && [ "$1" = clone ] && [ "$2" = https://github.com/vybestack/llxprt-jefe.git ]; then
            [ "$mode" = dirty-copy ] || exit 65
            /bin/mkdir -p "$PWD/${3##*/}"
            printf '%s\n' clean >"$state"
            mark clone
        else
            reject "$@"
        fi
        ;;
esac
