#!/bin/bash
set -euo pipefail

test "$#" = 5
test "$(uname -s)/$(uname -m)" = "Darwin/arm64"
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$4")")/$(basename "$4")
receipt=$(realpath "$(dirname "$5")")/$(basename "$5")
pin=526da0a7533d54bb71bd358dc3e3b26bcefe00bf
test ! -e "$receipt"
test -z "$(git -C "$here" status --porcelain --untracked-files=all)"
cargo +1.96.1 run --quiet --locked --offline --manifest-path "$here/../canonical-public-data/Cargo.toml" \
    --bin native-build-macos -- "$1" "$2" "$output" "$pin" \
    "$here/project.py" "$here/../status/macos-arm64.json" "$3"
python3 "$here/qualify.py" "$3" "$output/target/release/marmish-status-owner" "$receipt" \
    --framework-revision "$pin"
