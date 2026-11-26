#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=a882f1cfe4a65880cb038f3ea32ccd7e7b271591
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/discovery_project.py" "$here/macos-arm64.json" "$3"
python3 -B "$here/discovery_probe.py" "$3" "$receipt" \
    --candidate "$output/target/release/marmish" --framework-revision "$pin"
