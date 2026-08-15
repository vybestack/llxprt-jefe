#!/bin/sh
# Execute the host's signed macOS shell tools without copying their binaries.
set -eu
case "${0##*/}" in
    dd) exec /bin/dd "$@" ;;
    od) exec /usr/bin/od "$@" ;;
    stty) exec /bin/stty "$@" ;;
    tr) exec /usr/bin/tr "$@" ;;
    *) printf 'unsupported shell tool: %s\n' "${0##*/}" >&2; exit 64 ;;
esac
