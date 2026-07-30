#!/bin/sh
set -eu

real_git=/usr/bin/git
case "${1:-}" in
  fetch)
    exit 0
    ;;
  checkout)
    if [ "${2:-}" = "-B" ]; then
      "$real_git" reset --hard --quiet "${4:-HEAD}"
      exit 0
    fi
    ;;
esac
exec "$real_git" "$@"
