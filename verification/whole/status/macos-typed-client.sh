#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=ee4ba72af452aecb0d50c193c6b642552f8f0a79
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/typed_project.py" "$here/macos-arm64.json" "$3"
python3 -B "$here/typed_qualify.py" "$3" "$output/target/release" "$receipt" \
    --framework-revision "$pin"
