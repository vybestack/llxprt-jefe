#!/bin/sh
# Produce the bounded, local-only evidence for the first-agent tutorial.

set -eu
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
SCENARIO="$REPO_ROOT/dev-docs/tmux-scenarios/first-agent-tutorial.json"
OWNED_PATHS="home config socket fixture-repo private bin"

usage() {
    cat <<'EOF'
Usage:
  scripts/issue241-capture.sh capture --root ABSOLUTE_PATH --jefe-bin PATH --harness-bin PATH
  scripts/issue241-capture.sh cleanup --dry-run --root ABSOLUTE_PATH
  scripts/issue241-capture.sh cleanup --confirm --root ABSOLUTE_PATH
EOF
}

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

parse_capture() {
    ROOT=
    JEFE_BIN=
    HARNESS_BIN=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --root) ROOT=${2-}; shift 2 ;;
            --jefe-bin) JEFE_BIN=${2-}; shift 2 ;;
            --harness-bin) HARNESS_BIN=${2-}; shift 2 ;;
            *) fail "unknown capture argument: $1" ;;
        esac
    done
    [ -n "$ROOT" ] || fail "capture requires --root"
    [ -n "$JEFE_BIN" ] || fail "capture requires --jefe-bin"
    [ -n "$HARNESS_BIN" ] || fail "capture requires --harness-bin"
}

validate_new_root() {
    case "$ROOT" in
        /*) ;;
        *) fail "run root must be an absolute path" ;;
    esac
    [ "$ROOT" != "/" ] || fail "run root must not be filesystem root"
    case "$ROOT" in
        "$HOME"|"$HOME"/*) fail "run root must not be the normal home directory" ;;
    esac
    [ ! -e "$ROOT" ] && [ ! -L "$ROOT" ] || fail "run root must not exist"
}

write_manifest() {
    outcome=$1
    detail=$2
    {
        printf 'format_version=1\n'
        printf 'outcome=%s\n' "$outcome"
        printf 'jefe_commit=%s\n' "$JEFE_COMMIT"
        printf 'jefe_version=%s\n' "$JEFE_VERSION"
        printf 'scenario=%s\n' "dev-docs/tmux-scenarios/first-agent-tutorial.json"
        printf 'detail=%s\n' "$detail"
        for path in $OWNED_PATHS; do
            printf 'owned_path=%s\n' "$path"
        done
    } > "$ROOT/manifest.txt"
}

record_failure() {
    detail=$1
    printf '%s\n' "$detail" > "$ROOT/private/diagnostic.txt"
    if [ -f "$ROOT/evidence/error.txt" ]; then
        cat "$ROOT/evidence/error.txt" >> "$ROOT/private/diagnostic.txt"
    fi
    write_manifest failed "$detail"
    fail "$detail; private diagnostics retained under $ROOT/private"
}

shell_quote() {
    printf "'"
    printf '%s' "$1" | sed "s/'/'\\\\''/g"
    printf "'"
}

create_runtime_files() {
    mkdir -p "$ROOT/home" "$ROOT/config" "$ROOT/socket" \
        "$ROOT/fixture-repo/tutorial-agent" "$ROOT/private" \
        "$ROOT/evidence" "$ROOT/publication" "$ROOT/bin"
    printf 'jefe-issue241-capture-v1\n' > "$ROOT/.issue241-run"

    git -C "$ROOT/fixture-repo/tutorial-agent" init -q
    git -C "$ROOT/fixture-repo/tutorial-agent" config user.name "Tutorial User"
    git -C "$ROOT/fixture-repo/tutorial-agent" config user.email "tutorial@example.invalid"

    cat > "$ROOT/bin/llxprt" <<'EOF'
#!/bin/sh
case " ${*} " in
    *" --version "*) printf 'llxprt 0.0.0-tutorial\n'; exit 0 ;;
esac
printf 'tutorial-shim: ready\n'
while IFS= read -r line; do
    printf 'tutorial-shim: response: %s\n' "$line"
done
EOF
    chmod +x "$ROOT/bin/llxprt"

    {
        printf '#!/bin/sh\n'
        printf 'export HOME='; shell_quote "$ROOT/home"; printf '\n'
        printf 'export JEFE_SOCKET_PATH='; shell_quote "$ROOT/socket/jefe.sock"; printf '\n'
        printf 'export PATH='; shell_quote "$ROOT/bin:$PATH"; printf '\n'
        printf 'exec '; shell_quote "$JEFE_BIN"; printf ' "$@"\n'
    } > "$ROOT/bin/jefe-isolated"
    chmod +x "$ROOT/bin/jefe-isolated"
}

forbidden_literal() {
    forbidden_file=$1
    forbidden_value=$2
    forbidden_name=$3
    [ -n "$forbidden_value" ] || return 0
    if grep -F "$forbidden_value" "$forbidden_file" >/dev/null 2>&1; then
        record_failure "publication validation rejected $forbidden_name in $(basename "$forbidden_file")"
    fi
}

validate_capture() {
    file=$1
    forbidden_literal "$file" "${USER-}" "username"
    forbidden_literal "$file" "$(id -un 2>/dev/null || true)" "username"
    forbidden_literal "$file" "$(hostname 2>/dev/null || true)" "hostname"
    forbidden_literal "$file" "$NORMAL_HOME" "normal home path"
    forbidden_literal "$file" "$ROOT" "run-root path"
    forbidden_literal "$file" "$REPO_ROOT" "unrelated repository path"
    if grep -Ei '(gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|(token|password|secret)[[:space:]]*[=:][[:space:]]*[^[:space:]]+)' "$file" >/dev/null 2>&1; then
        record_failure "publication validation rejected credential-like content in $(basename "$file")"
    fi
}

publication_text() {
    source=$1
    target=$2
    sed -E \
        -e 's/pid:[0-9]+/pid:[redacted]/g' \
        -e 's/\[[^]]+ [0-9]{1,2}:[0-9]{2} [0-9]{1,2}-[A-Za-z]{3}-[0-9]{2}/[terminal status redacted]/g' \
        "$source" > "$target"
}

render_svg() {
    source=$1
    target=$2
    {
        printf '%s\n' '<svg xmlns="http://www.w3.org/2000/svg" width="800" height="594" viewBox="0 0 800 594" role="img">'
        printf '%s\n' '<rect width="800" height="594" fill="#000000"/>'
        printf '%s\n' '<text x="8" y="20" fill="#6a9955" font-family="monospace" font-size="14" xml:space="preserve">'
        row=0
        while IFS= read -r line || [ -n "$line" ]; do
            escaped=$(printf '%s' "$line" | sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g')
            y=$((20 + row * 18))
            printf '<tspan x="8" y="%s">%s</tspan>\n' "$y" "$escaped"
            row=$((row + 1))
        done < "$source"
        printf '%s\n' '</text>' '</svg>'
    } > "$target"
}

publish_captures() {
    for label in first-agent-dashboard first-agent-new-repository first-agent-new-agent \
        first-agent-terminal-ready first-agent-terminal-response first-agent-result; do
        source="$ROOT/evidence/$label.screen.txt"
        safe_text="$ROOT/private/$label.publication.txt"
        [ -f "$source" ] || record_failure "missing semantic capture: $label"
        publication_text "$source" "$safe_text"
        validate_capture "$safe_text"
        render_svg "$safe_text" "$ROOT/publication/$label.svg"
    done
}

capture_run() {
    parse_capture "$@"
    [ -x "$JEFE_BIN" ] || fail "jefe binary not found or not executable: $JEFE_BIN"
    [ -x "$HARNESS_BIN" ] || fail "harness binary not found or not executable: $HARNESS_BIN"
    NORMAL_HOME=$HOME
    validate_new_root
    mkdir "$ROOT" 2>/dev/null || fail "run root already exists or cannot be created: $ROOT"
    JEFE_COMMIT=$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || printf unknown)
    JEFE_VERSION=$($JEFE_BIN --version 2>/dev/null | head -n 1 || printf unknown)
    [ -n "$JEFE_VERSION" ] || JEFE_VERSION=unknown
    create_runtime_files
    write_manifest running "scenario not yet complete"

    if ! "$HARNESS_BIN" \
        --scenario "$SCENARIO" \
        --jefe-bin "$ROOT/bin/jefe-isolated" \
        --config "$ROOT/config" \
        --working-dir "$ROOT" \
        --session "jefe-issue241-$$" \
        --out-dir "$ROOT/evidence" \
        > "$ROOT/private/harness.stdout.txt" \
        2> "$ROOT/private/harness.stderr.txt"; then
        record_failure "first-agent tutorial scenario failed"
    fi
    publish_captures
    write_manifest success "scenario and publication validation completed"
    printf 'capture complete: %s\n' "$ROOT"
}

validate_cleanup_root() {
    case "$ROOT" in /*) ;; *) fail "run root must be an absolute path" ;; esac
    [ -d "$ROOT" ] && [ ! -L "$ROOT" ] || fail "run root must be a non-symlink directory"
    [ "$(cat "$ROOT/.issue241-run" 2>/dev/null || true)" = "jefe-issue241-capture-v1" ] || fail "run sentinel is missing or invalid"
    [ -f "$ROOT/manifest.txt" ] && [ ! -L "$ROOT/manifest.txt" ] || fail "run manifest is missing or unsafe"
}

validate_owned_path() {
    relative=$1
    case "$relative" in
        home|config|socket|fixture-repo|private|bin) ;;
        *) fail "manifest contains an unrecognized owned path: $relative" ;;
    esac
    [ ! -L "$ROOT/$relative" ] || fail "refusing symlink cleanup path: $relative"
}

cleanup_path() {
    relative=$1
    path="$ROOT/$relative"
    [ -e "$path" ] || return 0
    printf '%s\n' "$path"
    [ "$CLEANUP_MODE" = "dry-run" ] || find "$path" -depth -delete
}

cleanup_run() {
    CLEANUP_MODE=
    ROOT=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --dry-run) CLEANUP_MODE=dry-run; shift ;;
            --confirm) CLEANUP_MODE=confirm; shift ;;
            --root) ROOT=${2-}; shift 2 ;;
            *) fail "unknown cleanup argument: $1" ;;
        esac
    done
    [ -n "$CLEANUP_MODE" ] || fail "cleanup requires --dry-run or --confirm"
    [ -n "$ROOT" ] || fail "cleanup requires --root"
    validate_cleanup_root
    owned_count=0
    while IFS='=' read -r key relative; do
        [ "$key" = "owned_path" ] || continue
        validate_owned_path "$relative"
        owned_count=$((owned_count + 1))
    done < "$ROOT/manifest.txt"
    [ "$owned_count" -gt 0 ] || fail "manifest contains no owned paths"
    while IFS='=' read -r key relative; do
        [ "$key" = "owned_path" ] || continue
        cleanup_path "$relative"
    done < "$ROOT/manifest.txt"
}

COMMAND=${1-}
[ -n "$COMMAND" ] || { usage >&2; exit 2; }
shift
case "$COMMAND" in
    capture) capture_run "$@" ;;
    cleanup) cleanup_run "$@" ;;
    -h|--help) usage ;;
    *) usage >&2; fail "unknown command: $COMMAND" ;;
esac
