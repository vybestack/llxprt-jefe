#!/bin/sh
# Regenerate and verify the committed first-agent tutorial publication assets.

set -eu
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
CAPTURE_SCRIPT="$SCRIPT_DIR/issue241-capture.sh"
ASSET_DIR="$REPO_ROOT/docs/assets"
PROVENANCE="$ASSET_DIR/first-agent-tutorial.provenance"
ASSETS="first-agent-new-repository.svg first-agent-new-agent.svg first-agent-result.svg"
CONTRACT_PATHS="Cargo.toml Cargo.lock build.rs src dev-docs/tmux-scenarios/first-agent-tutorial.json scripts/issue241-capture.sh scripts/regenerate-first-agent-tutorial.sh"

usage() {
    cat <<'EOF'
Usage:
  scripts/regenerate-first-agent-tutorial.sh regenerate --root ABSOLUTE_PATH
  scripts/regenerate-first-agent-tutorial.sh regenerate --root ABSOLUTE_PATH --jefe-bin PATH --harness-bin PATH
  scripts/regenerate-first-agent-tutorial.sh check

Without explicit binary paths, regenerate builds all locked workspace binaries.
EOF
}

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

require_tool() {
    command -v "$1" >/dev/null 2>&1 || fail "required local tool not found on PATH: $1"
}

manifest_value() {
    key=$1
    file=$2
    sed -n "s/^$key=//p" "$file" | head -n 1
}

source_fingerprint() {
    files=$(git -C "$REPO_ROOT" ls-files -- $CONTRACT_PATHS)
    [ -n "$files" ] || fail "no tracked first-agent source contract files found"
    {
        printf '%s\n' "$files" | sort | while IFS= read -r file; do
            object=$(git -C "$REPO_ROOT" hash-object -- "$file")
            printf '%s  %s\n' "$object" "$file"
        done
    } | git -C "$REPO_ROOT" hash-object --stdin
}

parse_regenerate() {
    ROOT=
    JEFE_BIN=
    HARNESS_BIN=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --root) ROOT=${2-}; shift 2 ;;
            --jefe-bin) JEFE_BIN=${2-}; shift 2 ;;
            --harness-bin) HARNESS_BIN=${2-}; shift 2 ;;
            *) fail "unknown regenerate argument: $1" ;;
        esac
    done
    [ -n "$ROOT" ] || fail "regenerate requires --root"
    if [ -n "$JEFE_BIN" ] || [ -n "$HARNESS_BIN" ]; then
        [ -n "$JEFE_BIN" ] && [ -n "$HARNESS_BIN" ] || \
            fail "--jefe-bin and --harness-bin must be provided together"
    fi
}

prepare_binaries() {
    if [ -z "$JEFE_BIN" ]; then
        require_tool cargo
        (cd "$REPO_ROOT" && cargo build --workspace --all-features --locked --bins)
        JEFE_BIN="$REPO_ROOT/target/debug/jefe"
        HARNESS_BIN="$REPO_ROOT/target/debug/jefe-tmux-harness"
    fi
    [ -x "$JEFE_BIN" ] || fail "jefe binary not found or not executable: $JEFE_BIN"
    [ -x "$HARNESS_BIN" ] || fail "harness binary not found or not executable: $HARNESS_BIN"
}

write_provenance() {
    manifest=$ROOT/manifest.txt
    source_commit=$(manifest_value jefe_commit "$manifest")
    source_version=$(manifest_value jefe_version "$manifest")
    [ -n "$source_commit" ] || fail "capture manifest does not record jefe_commit"
    [ -n "$source_version" ] || fail "capture manifest does not record jefe_version"
    fingerprint=$(source_fingerprint)

    {
        printf 'format_version=1\n'
        printf 'source_commit=%s\n' "$source_commit"
        printf 'source_version=%s\n' "$source_version"
        printf 'source_fingerprint=%s\n' "$fingerprint"
        for asset in $ASSETS; do
            object=$(git -C "$REPO_ROOT" hash-object -- "$ROOT/publication/$asset")
            printf 'asset=%s:%s\n' "$asset" "$object"
        done
    } > "$ROOT/private/first-agent-tutorial.provenance"
}

validate_publication() {
    for asset in $ASSETS; do
        source=$ROOT/publication/$asset
        [ -f "$source" ] && [ ! -L "$source" ] || fail "missing publication asset: $asset"
    done
}

promote_publication() {
    for asset in $ASSETS; do
        cp "$ROOT/publication/$asset" "$ASSET_DIR/.$asset.tmp.$$"
    done
    cp "$ROOT/private/first-agent-tutorial.provenance" \
        "$ASSET_DIR/.first-agent-tutorial.provenance.tmp.$$"
    for asset in $ASSETS; do
        mv "$ASSET_DIR/.$asset.tmp.$$" "$ASSET_DIR/$asset"
    done
    mv "$ASSET_DIR/.first-agent-tutorial.provenance.tmp.$$" "$PROVENANCE"
}

remove_promotion_temps() {
    for asset in $ASSETS; do
        rm -f "$ASSET_DIR/.$asset.tmp.$$"
    done
    rm -f "$ASSET_DIR/.first-agent-tutorial.provenance.tmp.$$"
}

regenerate() {
    parse_regenerate "$@"
    require_tool git
    require_tool sed
    [ -x "$CAPTURE_SCRIPT" ] || fail "capture script not found or not executable: $CAPTURE_SCRIPT"
    prepare_binaries
    "$CAPTURE_SCRIPT" capture --root "$ROOT" --jefe-bin "$JEFE_BIN" --harness-bin "$HARNESS_BIN"
    validate_publication
    write_provenance
    trap remove_promotion_temps EXIT HUP INT TERM
    promote_publication
    trap - EXIT HUP INT TERM
    printf 'promoted first-agent tutorial assets from %s\n' "$ROOT"
    printf 'verify with: scripts/regenerate-first-agent-tutorial.sh check\n'
}

check_asset() {
    asset=$1
    line=$(grep -F "asset=$asset:" "$PROVENANCE" || true)
    [ -n "$line" ] || fail "provenance does not record asset: $asset"
    expected=${line#*:}
    actual=$(git -C "$REPO_ROOT" hash-object -- "$ASSET_DIR/$asset")
    [ "$actual" = "$expected" ] || fail "first-agent tutorial asset is stale: $asset"
}

check() {
    require_tool git
    require_tool sed
    require_tool grep
    [ -f "$PROVENANCE" ] && [ ! -L "$PROVENANCE" ] || \
        fail "first-agent tutorial provenance is missing: $PROVENANCE"
    expected=$(manifest_value source_fingerprint "$PROVENANCE")
    [ -n "$expected" ] || fail "provenance does not record source_fingerprint"
    actual=$(source_fingerprint)
    [ "$actual" = "$expected" ] || fail "first-agent tutorial source fingerprint is stale; regenerate the assets"
    for asset in $ASSETS; do
        check_asset "$asset"
    done
    printf 'first-agent tutorial assets match recorded provenance\n'
}

COMMAND=${1-}
[ -n "$COMMAND" ] || { usage >&2; exit 2; }
shift
case "$COMMAND" in
    regenerate) regenerate "$@" ;;
    check) [ "$#" -eq 0 ] || fail "check does not accept arguments"; check ;;
    -h|--help) usage ;;
    *) usage >&2; fail "unknown command: $COMMAND" ;;
esac
