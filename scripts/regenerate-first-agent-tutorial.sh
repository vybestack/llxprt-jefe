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
        printf '%s\n' "$files" | LC_ALL=C sort | while IFS= read -r file; do
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
            --root)
                [ "$#" -ge 2 ] || fail "--root requires a value"
                ROOT=$2
                shift 2
                ;;
            --jefe-bin)
                [ "$#" -ge 2 ] || fail "--jefe-bin requires a value"
                JEFE_BIN=$2
                shift 2
                ;;
            --harness-bin)
                [ "$#" -ge 2 ] || fail "--harness-bin requires a value"
                HARNESS_BIN=$2
                shift 2
                ;;
            *) fail "unknown regenerate argument: $1" ;;
        esac
    done
    [ -n "$ROOT" ] || fail "regenerate requires --root"
    case "$ROOT" in
        /*) ;;
        *) fail "regenerate requires an absolute --root" ;;
    esac
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
    [ -d "$ROOT/private" ] && [ ! -L "$ROOT/private" ] || \
        fail "capture private directory is missing or unsafe: $ROOT/private"
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

cleanup_promotion() {
    if [ "${PRESERVE_STAGE-0}" -ne 1 ] && [ -n "${STAGE_DIR-}" ] && \
        [ -d "$STAGE_DIR" ] && [ ! -L "$STAGE_DIR" ]; then
        find "$STAGE_DIR" -depth -delete
    fi
    if [ -n "${LOCK_DIR-}" ] && [ -d "$LOCK_DIR" ] && [ ! -L "$LOCK_DIR" ]; then
        rmdir "$LOCK_DIR" 2>/dev/null || true
    fi
}

abort_promotion() {
    trap - HUP INT TERM
    if [ "${PROMOTION_STARTED-0}" -eq 1 ] && ! restore_publication; then
        PRESERVE_STAGE=1
        printf 'tutorial promotion interrupted; rollback incomplete; recover backups from %s/backup\n' \
            "$STAGE_DIR" >&2
    fi
    exit 1
}

prepare_promotion() {
    LOCK_DIR="$ASSET_DIR/.first-agent-tutorial.lock"
    mkdir "$LOCK_DIR" 2>/dev/null || fail "another regeneration owns promotion: $LOCK_DIR"
    PROMOTION_STARTED=0
    PRESERVE_STAGE=0
    trap cleanup_promotion EXIT
    trap abort_promotion HUP INT TERM
    STAGE_DIR=$(mktemp -d "$ASSET_DIR/.first-agent-tutorial.XXXXXX") || \
        fail "cannot create tutorial promotion staging directory"
    mkdir "$STAGE_DIR/new" "$STAGE_DIR/backup"

    for asset in $ASSETS; do
        cp "$ROOT/publication/$asset" "$STAGE_DIR/new/$asset"
    done
    cp "$ROOT/private/first-agent-tutorial.provenance" \
        "$STAGE_DIR/new/first-agent-tutorial.provenance"
    for file in $ASSETS first-agent-tutorial.provenance; do
        target=$ASSET_DIR/$file
        [ -f "$target" ] && [ ! -L "$target" ] || fail "promotion target is missing or unsafe: $target"
        cp "$target" "$STAGE_DIR/backup/$file"
    done
}

restore_publication() {
    restored=1
    for file in $ASSETS first-agent-tutorial.provenance; do
        if ! cp "$STAGE_DIR/backup/$file" "$ASSET_DIR/$file"; then
            printf 'failed to restore tutorial asset: %s\n' "$file" >&2
            restored=0
        fi
    done
    [ "$restored" -eq 1 ]
}

promote_publication() {
    prepare_promotion
    PROMOTION_STARTED=1
    for file in $ASSETS first-agent-tutorial.provenance; do
        if ! mv "$STAGE_DIR/new/$file" "$ASSET_DIR/$file"; then
            if restore_publication; then
                fail "tutorial promotion failed; restored every committed asset"
            fi
            PRESERVE_STAGE=1
            fail "tutorial promotion failed and rollback was incomplete; recover backups from $STAGE_DIR/backup"
        fi
    done
    PROMOTION_STARTED=0
    cleanup_promotion
    trap - EXIT HUP INT TERM
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
    promote_publication
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
