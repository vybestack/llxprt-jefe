#!/bin/sh
# Regenerate and verify the committed first-agent tutorial publication assets.

set -eu
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
DRIVER="$SCRIPT_DIR/run-scenario-manifest.py"
PUBLISHER="$SCRIPT_DIR/publish-first-agent-tutorial.py"
SCENARIO="dev-docs/tmux-scenarios/first-agent-tutorial.json"
REPORT_NAME="dev-docs__tmux-scenarios__first-agent-tutorial.json"
ASSET_DIR="$REPO_ROOT/docs/assets"
PROVENANCE="$ASSET_DIR/first-agent-tutorial.provenance"
ASSETS="first-agent-new-repository.svg first-agent-new-agent.svg first-agent-result.svg first-agent-code-puppy.svg first-agent-issues.svg first-agent-issue-send.svg first-agent-pull-request.svg first-agent-pr-merge.svg"
CONTRACT_PATHS="Cargo.toml Cargo.lock build.rs src dev-docs/testing/scenario-execution-manifest.json dev-docs/tmux-scenarios/first-agent-tutorial.json scripts/run-scenario-manifest.py scripts/publish-first-agent-tutorial.py scripts/first-agent-tutorial-*-shim.sh scripts/regenerate-first-agent-tutorial.sh"
SENTINEL="jefe-first-agent-tutorial-v2"

usage() {
    cat <<'EOF'
Usage:
  scripts/regenerate-first-agent-tutorial.sh regenerate --root ABSOLUTE_PATH
  scripts/regenerate-first-agent-tutorial.sh regenerate --root ABSOLUTE_PATH --tmux-scenario PATH --jefe PATH --probe PATH --jsp-fixture PATH --shim PATH
  scripts/regenerate-first-agent-tutorial.sh cleanup --dry-run --root ABSOLUTE_PATH
  scripts/regenerate-first-agent-tutorial.sh cleanup --confirm --root ABSOLUTE_PATH
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
    files=$(git -C "$REPO_ROOT" ls-files --cached --others --exclude-standard -- $CONTRACT_PATHS)
    [ -n "$files" ] || fail "no first-agent source contract files found"
    {
        printf '%s\n' "$files" | LC_ALL=C sort -u | while IFS= read -r file; do
            [ -f "$REPO_ROOT/$file" ] || continue
            object=$(git -C "$REPO_ROOT" hash-object -- "$file")
            printf '%s  %s\n' "$object" "$file"
        done
    } | git -C "$REPO_ROOT" hash-object --stdin
}

parse_regenerate() {
    ROOT=
    TMUX_SCENARIO=
    JEFE=
    PROBE=
    JSP_FIXTURE=
    SHIM=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --root|--tmux-scenario|--jefe|--probe|--jsp-fixture|--shim)
                flag=$1
                [ "$#" -ge 2 ] || fail "$flag requires a value"
                case "$flag" in
                    --root) ROOT=$2 ;;
                    --tmux-scenario) TMUX_SCENARIO=$2 ;;
                    --jefe) JEFE=$2 ;;
                    --probe) PROBE=$2 ;;
                    --jsp-fixture) JSP_FIXTURE=$2 ;;
                    --shim) SHIM=$2 ;;
                esac
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
    [ ! -e "$ROOT" ] && [ ! -L "$ROOT" ] || fail "regenerate root must not already exist: $ROOT"
    supplied=0
    for value in "$TMUX_SCENARIO" "$JEFE" "$PROBE" "$JSP_FIXTURE" "$SHIM"; do
        [ -z "$value" ] || supplied=$((supplied + 1))
    done
    [ "$supplied" -eq 0 ] || [ "$supplied" -eq 5 ] || \
        fail "all explicit binary paths must be provided together"
}

prepare_binaries() {
    if [ -z "$JEFE" ]; then
        require_tool cargo
        (cd "$REPO_ROOT" && cargo build --workspace --all-features --locked --bins)
        TMUX_SCENARIO="$REPO_ROOT/target/debug/tmux_scenario"
        JEFE="$REPO_ROOT/target/debug/jefe"
        PROBE="$REPO_ROOT/target/debug/jefe-harness-probe"
        JSP_FIXTURE="$REPO_ROOT/target/debug/jefe-jsp-llxprt-fixture"
        SHIM="$REPO_ROOT/target/debug/jefe-capture-shim"
    fi
    for binary in "$TMUX_SCENARIO" "$JEFE" "$PROBE" "$JSP_FIXTURE" "$SHIM"; do
        [ -x "$binary" ] || fail "required binary not found or not executable: $binary"
    done
}

write_run_manifest() {
    report=$ROOT/evidence/$REPORT_NAME
    [ -f "$report" ] || fail "canonical tutorial report is missing: $report"
    {
        printf 'format_version=2\n'
        printf 'jefe_commit=%s\n' "$(git -C "$REPO_ROOT" rev-parse HEAD)"
        printf 'jefe_version=%s\n' "$($JEFE --version | head -n 1)"
        printf 'scenario=%s\n' "$SCENARIO"
        printf 'runner=tmux_scenario\n'
        printf 'report_sha256=%s\n' "$(git -C "$REPO_ROOT" hash-object -- "$report")"
    } > "$ROOT/manifest.txt"
}

write_provenance() {
    manifest=$ROOT/manifest.txt
    [ -d "$ROOT/private" ] && [ ! -L "$ROOT/private" ] || \
        fail "publication private directory is missing or unsafe: $ROOT/private"
    source_commit=$(manifest_value jefe_commit "$manifest")
    source_version=$(manifest_value jefe_version "$manifest")
    [ -n "$source_commit" ] || fail "run manifest does not record jefe_commit"
    [ -n "$source_version" ] || fail "run manifest does not record jefe_version"
    fingerprint=$(source_fingerprint)
    {
        printf 'format_version=2\n'
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
        [ ! -L "$target" ] || fail "promotion target is unsafe: $target"
        if [ -f "$target" ]; then
            cp "$target" "$STAGE_DIR/backup/$file"
        elif [ -e "$target" ]; then
            fail "promotion target is unsafe: $target"
        fi
    done
}

restore_publication() {
    restored=1
    for file in $ASSETS first-agent-tutorial.provenance; do
        if [ -f "$STAGE_DIR/backup/$file" ]; then
            cp "$STAGE_DIR/backup/$file" "$ASSET_DIR/$file" || restored=0
        elif [ -L "$ASSET_DIR/$file" ] || ! rm -f "$ASSET_DIR/$file"; then
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
    require_tool python3
    [ -x "$DRIVER" ] || fail "manifest driver not found or not executable: $DRIVER"
    [ -x "$PUBLISHER" ] || fail "report publisher not found or not executable: $PUBLISHER"
    prepare_binaries
    mkdir -m 700 "$ROOT"
    printf '%s\n' "$SENTINEL" > "$ROOT/.first-agent-tutorial-run"
    "$DRIVER" \
        --platform macos \
        --scenario "$SCENARIO" \
        --tmux-scenario "$TMUX_SCENARIO" \
        --jefe "$JEFE" \
        --probe "$PROBE" \
        --jsp-fixture "$JSP_FIXTURE" \
        --shim "$SHIM" \
        --reports "$ROOT/evidence"
    "$PUBLISHER" --report "$ROOT/evidence/$REPORT_NAME" --root "$ROOT"
    write_run_manifest
    validate_publication
    write_provenance
    promote_publication
    printf 'promoted first-agent tutorial assets from %s\n' "$ROOT"
    printf 'verify with: scripts/regenerate-first-agent-tutorial.sh check\n'
}

validate_cleanup_root() {
    [ -n "$ROOT" ] || fail "cleanup requires --root"
    case "$ROOT" in /*) ;; *) fail "cleanup requires an absolute --root" ;; esac
    [ "$ROOT" != "/" ] && [ "$ROOT" != "$HOME" ] || fail "refusing unsafe cleanup root: $ROOT"
    [ -d "$ROOT" ] && [ ! -L "$ROOT" ] || fail "cleanup root is missing or unsafe: $ROOT"
    [ "$(cat "$ROOT/.first-agent-tutorial-run" 2>/dev/null || true)" = "$SENTINEL" ] || \
        fail "cleanup sentinel is missing or invalid"
}

cleanup() {
    mode=
    ROOT=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --dry-run|--confirm) [ -z "$mode" ] || fail "choose one cleanup mode"; mode=$1; shift ;;
            --root) [ "$#" -ge 2 ] || fail "--root requires a value"; ROOT=$2; shift 2 ;;
            *) fail "unknown cleanup argument: $1" ;;
        esac
    done
    [ -n "$mode" ] || fail "cleanup requires --dry-run or --confirm"
    validate_cleanup_root
    if [ "$mode" = "--dry-run" ]; then
        printf 'would remove owned tutorial run root: %s\n' "$ROOT"
        return
    fi
    find "$ROOT" -depth -delete
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
    require_tool grep
    [ -f "$PROVENANCE" ] && [ ! -L "$PROVENANCE" ] || \
        fail "first-agent tutorial provenance is missing: $PROVENANCE"
    expected=$(manifest_value source_fingerprint "$PROVENANCE")
    [ -n "$expected" ] || fail "provenance does not record source_fingerprint"
    actual=$(source_fingerprint)
    [ "$actual" = "$expected" ] || fail "first-agent tutorial source fingerprint is stale; regenerate the assets"
    for asset in $ASSETS; do check_asset "$asset"; done
    printf 'first-agent tutorial assets match recorded provenance\n'
}

COMMAND=${1-}
[ -n "$COMMAND" ] || { usage >&2; exit 2; }
shift
case "$COMMAND" in
    regenerate) regenerate "$@" ;;
    cleanup) cleanup "$@" ;;
    check) [ "$#" -eq 0 ] || fail "check does not accept arguments"; check ;;
    -h|--help) usage ;;
    *) usage >&2; fail "unknown command: $COMMAND" ;;
esac
