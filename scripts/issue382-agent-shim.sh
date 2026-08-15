#!/bin/sh
set -eu

name=${0##*/}
mode=${HARNESS_ISSUE382_PROBE_MODE:-standard}

if [ "$mode" = negative ]; then
    case "$name" in
        codex)
            printf '\377\376\377\377\n'
            exit 1
            ;;
        llxprt)
            printf '{"identity":"0.10"}GARBAGE\n'
            exit 0
            ;;
        *)
            printf 'unexpected negative-probe executable: %s\n' "$name" >&2
            exit 64
            ;;
    esac
fi

if [ "$mode" = resolver ]; then
    if [ "$name" = claude ]; then
        printf '2.1.212 (Claude Code)\n'
        exit 0
    fi
    printf 'unexpected resolver executable: %s\n' "$name" >&2
    exit 64
fi

if [ "$mode" = status-cartesian ]; then
    case "$name" in
        llxprt) printf 'code-puppy 0.0.634\n' ;;
        codex) printf 'codex-cli 0.142.0\n' ;;
        code-puppy) printf 'llxprt 0.10.0\n' ;;
        *)
            printf 'unexpected status-cartesian executable: %s\n' "$name" >&2
            exit 64
            ;;
    esac
    exit 0
fi

if [ "$name" = npm ]; then
    workspace=${0%/bin/npm}
    printf '%s\n' "$*" >> "$workspace/npm-argv.txt"
    if [ "$1" = view ]; then
        printf '2.1.212\n'
        exit 0
    fi
    python3 -c '
from pathlib import Path
binary = Path("node_modules/.bin/claude")
binary.parent.mkdir(parents=True)
binary.write_text("""#!/bin/sh
if [ "${1:-}" = "--version" ]; then
    printf "2.1.212 (Claude Code)\\n"
    exit 0
fi
printf "%s\\n" "$*" > "$ISSUE382_CLAUDE_WITNESS"
""")
binary.chmod(0o755)
'
    exit 0
fi

case "$name" in
    llxprt)
        printf '0.10.0-nightly.260720.d69bda66a\n'
        ;;
    codex)
        printf 'codex-cli 0.142.0\n'
        ;;
    code-puppy)
        printf '0.0.634\n'
        ;;
    claude)
        printf '2.1.212 (Claude Code)\n'
        ;;
    podman)
        exit 127
        ;;
    *)
        printf 'unexpected issue382 fixture executable: %s\n' "$name" >&2
        exit 64
        ;;
esac
