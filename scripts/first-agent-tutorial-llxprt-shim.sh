#!/bin/sh
set -eu
case " ${*} " in
    *" --version "*) printf '0.0.0-tutorial\n'; exit 0 ;;
esac
printf 'tutorial-shim: ready\n'
case "$*" in
    *"#352"*) printf 'tutorial-shim: received issue 352\n' ;;
esac
while IFS= read -r line; do
    printf 'tutorial-shim: response: %s\n' "$line"
done
