#!/bin/sh
set -eu
root=
[ "${1-}" = "capture" ] || { printf 'expected capture command\n' >&2; exit 1; }
shift
while [ "$#" -gt 0 ]; do
    case "$1" in
        --root) root=$2; shift 2 ;;
        --jefe-bin|--harness-bin) shift 2 ;;
        *) printf 'unknown capture argument: %s\n' "$1" >&2; exit 1 ;;
    esac
done
[ -n "$root" ] || { printf 'capture requires --root\n' >&2; exit 1; }
mkdir -p "$root/publication"
[ -n "${OMIT_PRIVATE-}" ] || mkdir -p "$root/private"
printf 'jefe_commit=%s\n' "$(git rev-parse HEAD)" > "$root/manifest.txt"
printf 'jefe_version=jefe 9.9.9-fixture\n' >> "$root/manifest.txt"
for asset in first-agent-new-repository.svg first-agent-new-agent.svg first-agent-result.svg \
    first-agent-code-puppy.svg first-agent-issues.svg first-agent-issue-send.svg \
    first-agent-pull-request.svg first-agent-pr-merge.svg; do
    [ "${OMIT_ASSET-}" = "$asset" ] || \
        printf '<svg>%s</svg>\n' "$asset" > "$root/publication/$asset"
done
