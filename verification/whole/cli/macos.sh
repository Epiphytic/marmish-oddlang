#!/bin/bash
set -euo pipefail

test "$#" = 3
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$3")")/$(basename "$3")
pin=c01279eb61ed9e86b51b2eeaf0c5b744397c74a3
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/project.py" "$here/transcript.json"
python3 "$here/replay.py" verify "$output/target/release"
