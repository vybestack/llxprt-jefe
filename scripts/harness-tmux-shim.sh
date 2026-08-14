#!/bin/sh
set -eu

real_tmux="${0%/*}/tmux-real"
set +e
output=$("$real_tmux" "$@" 2>&1)
status=$?
set -e

if [ "$status" -eq 0 ]; then
    if [ -n "$output" ]; then
        printf '%s\n' "$output"
    fi
    exit 0
fi

case "$output" in
    *"error connecting to "*"(No such file or directory)"*)
        case " $* " in
            *" list-sessions "*|*" list-windows "*) exit 0 ;;
        esac
        ;;
esac
printf '%s\n' "$output" >&2
exit "$status"
