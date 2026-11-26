#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
package="$root/crates/marmish-stream"
stage=$(mktemp -d "$package/.oddlang-stream.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT
pin=$(cat "$root/oddlang/stream.sha256")
"${ODDLANG:-oddlang}" build rust-tree "$root/oddlang/stream" "$stage/generated" --bundle-sha256 "$pin"

if [[ -L "$package/src" ]]; then
    printf '%s\n' "Refusing to replace a symlink at $package/src" >&2
    exit 1
fi
if [[ -e "$package/src" ]]; then
    diff -r "$stage/generated/src" "$package/src"
else
    mv -n -- "$stage/generated/src" "$package/"
    if [[ -d "$stage/generated/src" ]]; then
        diff -r "$stage/generated/src" "$package/src"
    fi
fi
printf '%s\n' "Verified generated Rust in $package/src"
