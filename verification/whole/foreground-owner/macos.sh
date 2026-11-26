#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=07b43dc3572472f56a5617c303dc27f9e0a2047a
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/project.py" "$here/../relay/owner-tls-macos-arm64.json" "$3"
python3 "$here/qualify.py" "$3" "$output" "$receipt" --framework-revision "$pin"
