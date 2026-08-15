#!/bin/sh
set -eu

case "${1:-}" in
  --version) printf '0.10.0\n' ;;
  --help) printf '%s\n' '--prompt-interactive --profile-load --sandbox --sandbox-engine --yolo --approval-mode --continue' ;;
  *) exec llxprt-agent "$@" ;;
esac
