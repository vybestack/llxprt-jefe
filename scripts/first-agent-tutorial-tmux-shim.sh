#!/bin/sh
set -eu

real_tmux="${0%/*}/tmux-real"
printf 'tmux %s\n' "$*" >> "${TUTORIAL_TMUX_AUDIT:?}"
set +e
output=$("$real_tmux" "$@" 2>&1)
status=$?
set -e
printf 'exit %s\n' "$status" >> "$TUTORIAL_TMUX_AUDIT"

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
            *) printf 'no server running on tutorial socket\n' >&2 ;;
        esac
        ;;
    *)
        printf '%s\n' "$output" >&2
        ;;
esac
exit "$status"
