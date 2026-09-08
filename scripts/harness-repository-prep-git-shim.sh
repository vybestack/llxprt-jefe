#!/bin/sh
set -eu

mode=${HARNESS_GIT_MODE:?HARNESS_GIT_MODE is required}
state=${HARNESS_GIT_STATE:?HARNESS_GIT_STATE is required}
audit=${HARNESS_GIT_AUDIT:?HARNESS_GIT_AUDIT is required}
diaglog=$audit.diaglog

printf 'INV pwd=%s argc=%s mode=<%s> state=<%s> audit=<%s> args=<%s>\n' "$PWD" "$#" "$mode" "$state" "$audit" "$*" >>"$diaglog" 2>>"$diaglog" || true

trap 'printf "EXIT=%s pwd=%s args=<%s>\n" "$?" "$PWD" "$*" >>"$diaglog" 2>>"$diaglog" || true' EXIT

mark() {
    : >"$audit.$1"
}

reject() {
    : >"$audit.rejected"
    bin=$(/bin/ps -p "$PPID" -o command= 2>/dev/null | /usr/bin/awk '{print $1}')
    pa=$(/bin/ps -p "$PPID" -o comm= 2>/dev/null || true)
    sb1=$(/usr/bin/strings "$bin" 2>/dev/null | /usr/bin/grep -c 'https://github.com/' | /usr/bin/tr -d '[:space:]' || true)
    sb2=$(/usr/bin/strings "$bin" 2>/dev/null | /usr/bin/grep -c 'git@github' | /usr/bin/tr -d '[:space:]' || true)
    printf 'CF2:' >&2
    printf ' <%s>' "$@" >&2
    printf ' PP=%s PA=%s SB=%s/%s PT=%s' "$PPID" "${pa:-none}" "${sb1:-x}/${sb2:-x}" "${PATH%%:*}" >&2
    printf '\n' >&2
    printf 'REJECT pwd=%s argc=%s args=<%s> bin=<%s> pa=<%s> sb=%s/%s\n' "$PWD" "$#" "$*" "$bin" "$pa" "$sb1" "$sb2" >>"$diaglog" 2>>"$diaglog" || true
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
            printf 'CLONE start pwd=%s argc=%s dest=<%s> mode=<%s>\n' "$PWD" "$#" "$3" "$mode" >>"$diaglog" 2>>"$diaglog" || true
            if [ "$mode" = dirty-copy ]; then
                :
            else
                printf 'CLONEFAIL:mode:<%s>\n' "$mode" >&2
                printf 'CLONE mode-mismatch mode=<%s>\n' "$mode" >>"$diaglog" 2>>"$diaglog" || true
                exit 65
            fi
            if err=$(/bin/mkdir -p "$PWD/${3##*/}" 2>&1); then
                :
            else
                printf 'CLONEFAIL:mkdir:<%s>\n' "$err" >&2
                printf 'CLONE mkdir-failed pwd=<%s> dest=<%s> err=<%s>\n' "$PWD" "${3##*/}" "$err" >>"$diaglog" 2>>"$diaglog" || true
                exit 1
            fi
            if err=$(printf '%s\n' clean >"$state" 2>&1); then
                :
            else
                printf 'CLONEFAIL:state:<%s>\n' "$err" >&2
                printf 'CLONE state-failed state=<%s> err=<%s>\n' "$state" "$err" >>"$diaglog" 2>>"$diaglog" || true
                exit 1
            fi
            if err=$(: >"$audit.clone" 2>&1); then
                :
            else
                printf 'CLONEFAIL:mark:<%s>\n' "$err" >&2
                printf 'CLONE mark-failed audit=<%s> err=<%s>\n' "$audit" "$err" >>"$diaglog" 2>>"$diaglog" || true
                exit 1
            fi
            printf 'CLONE ok\n' >>"$diaglog" 2>>"$diaglog" || true
        else
            reject "$@"
        fi
        ;;
esac
