#!/bin/sh
set -eu

real_git=/usr/bin/git
case "${1:-}" in
  fetch)
    exit 0
    ;;
  checkout)
    shift
    if [ "${1:-}" = "-B" ]; then
      "$real_git" reset --hard --quiet "${3:?missing revision}"
      exit 0
    fi
    ;;
esac
exec "$real_git" "$@"
