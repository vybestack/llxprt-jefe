#!/bin/sh
set -eu
case " ${*} " in
    *" --help "*) printf '%s\n' 'usage: code-puppy [-i] [--model MODEL] [--yolo {true,false}]'; exit 0 ;;
    *" --version "*) printf '0.0.0-tutorial\n'; exit 0 ;;
esac
printf 'puppy-shim: ready\n'
case "$*" in
    *"#352"*) printf 'puppy-shim: received issue 352\n' ;;
esac
while IFS= read -r line; do
    printf 'puppy-shim: response: %s\n' "$line"
done
