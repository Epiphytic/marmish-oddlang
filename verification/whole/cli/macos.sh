#!/bin/bash
set -euo pipefail

test "$#" = 3
here=$(cd "$(dirname "$0")" && pwd -P)
output=$(realpath "$(dirname "$3")")/$(basename "$3")
pin=c01279eb61ed9e86b51b2eeaf0c5b744397c74a3
bash "$here/../native-build/macos.sh" "$1" "$2" "$output" "$pin" \
    "$here/../canonical-public-data/src/bin/cli-project.rs" "$here/transcript.json"
cargo +1.96.1 run --quiet --locked --offline --manifest-path "$here/../canonical-public-data/Cargo.toml" \
    --bin cli-transcript -- verify "$output/target/release"
