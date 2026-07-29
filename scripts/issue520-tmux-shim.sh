#!/bin/sh
set -eu

case " $* " in
  *" -V "*)
    printf 'tmux 3.4
'
    exit 0
    ;;
esac
if [ "${1:-}" = "display-message" ] && [ "${2:-}" = "-p" ]; then
  printf '12345|tmux 3.4
'
  exit 0
fi

case " $* " in
  *" new-session "*)
    while [ "$#" -gt 0 ]; do
      if [ "$1" = "-c" ]; then
        shift
        cd "${1:?missing cwd}"
      elif [ "$1" = "env" ]; then
        shift
        while [ "${1:-}" = "-u" ]; do
          shift 2
        done
        exec "$@"
      fi
      shift
    done
    ;;
esac
exit 64
