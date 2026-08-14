#!/bin/sh
set -eu

emit() {
    while IFS= read -r line || [ -n "$line" ]; do
        printf '%s\n' "$line"
    done
}

if [ "$#" -eq 0 ]; then
    emit
    exit 0
fi
for file in "$@"; do
    case "$file" in
        -*) exit 64 ;;
    esac
    [ -f "$file" ] || exit 1
    emit < "$file"
done
