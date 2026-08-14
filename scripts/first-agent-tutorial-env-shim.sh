#!/bin/sh
set -eu

while [ "$#" -gt 0 ]; do
    case "$1" in
        -u)
            [ "$#" -ge 2 ] || exit 64
            unset "$2"
            shift 2
            ;;
        *=*)
            export "$1"
            shift
            ;;
        *)
            exec "$@"
            ;;
    esac
done
exit 64
