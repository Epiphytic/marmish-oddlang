#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=ca97ed84eea8d3d3120c8c903e2f5e4da9cfdd74
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/project.py" "$here/../status/macos-arm64.json" "$3"
python3 "$here/qualify.py" "$3" "$output/target/release/marmish-status-owner" "$receipt" \
    --framework-revision "$pin"
