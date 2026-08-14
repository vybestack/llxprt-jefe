#!/bin/sh
set -eu

# Emulate the Windows npm shim, which spawns Node and takes seconds to answer
# --version. Availability remains in flight when Shift+S is pressed.
sleep 9
exec /bin/false
