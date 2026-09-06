#!/bin/sh
set -eu

ALPHA_ONE_TARGET='jefe-agent_widgets_one'
ALPHA_FOUR_TARGET='jefe-agent_widgets_four'
INVOCATION=$*

unexpected() {
  printf 'unexpected tmux invocation: %s\n' "$INVOCATION" >&2
  exit 64
}

if [ "$#" -lt 5 ] ||
  [ "$1" != '-f' ] ||
  [ "$2" != '/dev/null' ] ||
  [ "$3" != '-S' ] ||
  [ -z "$4" ]; then
  unexpected
fi

shift 4

case "$#" in
  1)
    if [ "$1" = '-V' ]; then
      printf 'tmux 3.4\n'
      exit 0
    fi
    ;;
  3)
    if [ "$1" = 'has-session' ] && [ "$2" = '-t' ]; then
      case "$3" in
        "$ALPHA_ONE_TARGET" | "$ALPHA_FOUR_TARGET") exit 0 ;;
      esac
    elif [ "$1" = 'list-sessions' ] &&
      [ "$2" = '-F' ] &&
      [ "$3" = '#{session_name}' ]; then
      # Independent tmux snapshots can disagree during session create/destroy races.
      printf '%s\n' "$ALPHA_ONE_TARGET"
      exit 0
    fi
    ;;
  4)
    if [ "$1" = 'list-windows' ] &&
      [ "$2" = '-a' ] &&
      [ "$3" = '-F' ] &&
      [ "$4" = '#{session_name}:#{window_name}' ]; then
      exit 0
    elif [ "$1" = 'list-panes' ] &&
      [ "$2" = '-a' ] &&
      [ "$3" = '-F' ] &&
      [ "$4" = '#{session_name}:#{window_index}:#{pane_dead}' ]; then
      printf '%s:0:0\n' "$ALPHA_ONE_TARGET"
      exit 0
    fi
    ;;
  5)
    if [ "$1" = 'list-panes' ] &&
      [ "$2" = '-t' ] &&
      [ "$4" = '-F' ] &&
      [ "$5" = '#{pane_dead}' ]; then
      case "$3" in
        "$ALPHA_ONE_TARGET" | "$ALPHA_FOUR_TARGET") printf '0\n'; exit 0 ;;
      esac
    elif [ "$1" = 'list-windows' ] &&
      [ "$2" = '-t' ] &&
      [ "$3" = "$ALPHA_ONE_TARGET" ] &&
      [ "$4" = '-F' ] &&
      [ "$5" = '#{window_name}' ]; then
      exit 0
    fi
    ;;
esac

unexpected
