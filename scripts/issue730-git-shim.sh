#!/bin/sh
# Strict git shim for dev-docs/tmux-scenarios/issue730/agent-row-status-glyphs.json.
#
# The harness PATH is ${workspace}/bin only (src/harness/v1/env.rs), so this
# installed shim is the only git the binary can see. It answers exactly the
# display-probe vocabulary of src/git_info/mod.rs (`rev-parse --abbrev-ref
# HEAD`, `status --porcelain=v1 -z`) for the four fixture work dirs, always
# branch `main` with a dirty tree, and refuses every other invocation with
# exit 64 so an unexpected probe fails loudly instead of silently degrading
# the suffix the scenario asserts.
set -eu

[ "${1:-}" = "-C" ] || {
    printf 'unexpected git invocation: %s\n' "$*" >&2
    exit 64
}
work_dir=${2:?missing git work directory}
shift 2

case "$work_dir:$*" in
    one:"rev-parse --abbrev-ref HEAD"|*/one:"rev-parse --abbrev-ref HEAD"|\
    two:"rev-parse --abbrev-ref HEAD"|*/two:"rev-parse --abbrev-ref HEAD"|\
    three:"rev-parse --abbrev-ref HEAD"|*/three:"rev-parse --abbrev-ref HEAD"|\
    four:"rev-parse --abbrev-ref HEAD"|*/four:"rev-parse --abbrev-ref HEAD")
        printf 'main\n'
        ;;
    one:"status --porcelain=v1 -z"|*/one:"status --porcelain=v1 -z"|\
    two:"status --porcelain=v1 -z"|*/two:"status --porcelain=v1 -z"|\
    three:"status --porcelain=v1 -z"|*/three:"status --porcelain=v1 -z"|\
    four:"status --porcelain=v1 -z"|*/four:"status --porcelain=v1 -z")
        printf ' M seeded-change.txt\0'
        ;;
    *)
        printf 'unexpected git invocation in %s: %s\n' "$work_dir" "$*" >&2
        exit 64
        ;;
esac
