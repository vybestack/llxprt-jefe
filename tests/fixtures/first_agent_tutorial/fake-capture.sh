#!/bin/sh
set -eu
root=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --root) root=$2; shift 2 ;;
        *) shift ;;
    esac
done
mkdir -p "$root/publication" "$root/private"
printf 'jefe_commit=%s\n' "$(git rev-parse HEAD)" > "$root/manifest.txt"
printf 'jefe_version=jefe 9.9.9-fixture\n' >> "$root/manifest.txt"
for asset in first-agent-new-repository.svg first-agent-new-agent.svg first-agent-result.svg; do
    [ "${OMIT_ASSET-}" = "$asset" ] || \
        printf '<svg>%s</svg>\n' "$asset" > "$root/publication/$asset"
done
