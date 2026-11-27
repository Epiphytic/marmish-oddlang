#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=c94b678a9672a75df9fe761dad62cfba9aaff33f
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/../canonical-public-data/src/bin/status-project.rs" "$here/macos-arm64.json" "$3"
python3 "$here/qualify.py" "$3" "$output/target/release" "$receipt" \
    --framework-revision "$pin"
