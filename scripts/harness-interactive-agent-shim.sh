#!/bin/sh
set -eu

if [ "${1:-}" = "--version" ]; then
    printf '0.0.0-harness\n'
    exit 0
fi

case "$PWD" in
    */fixture-repo-*/*) printf '%s\n' 'issue364-agent-ready' ;;
    *) printf '%s\n' 'issue222-agent-ready' ;;
esac
while :; do
    if IFS= read -r line; then
        if [ "$line" = "for i in \$(seq 1 60); do printf 'line %d\\n' \$i; done" ]; then
            line_number=1
            while [ "$line_number" -le 60 ]; do
                printf 'line %d\n' "$line_number"
                line_number=$((line_number + 1))
            done
        else
            printf '%s\n' "$line"
        fi
    else
        /bin/sleep 1
    fi
done
