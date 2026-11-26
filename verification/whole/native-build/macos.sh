#!/bin/bash
set -euo pipefail

test "$#" = 6 || test "$#" = 7
framework=$(realpath "$1")
original=$(realpath "$2")
output=$(realpath "$(dirname "$3")")/$(basename "$3")
framework_pin=$4
project=$(realpath "$5")
transcript=$(realpath "$6")
oracle=""
if [[ $# == 7 ]]; then
    oracle=$(realpath "$7")
fi
here=$(dirname "$project")
companion=$(git -C "$here" rev-parse --show-toplevel)
toolchain=1.97.1
test "$(git -C "$framework" rev-parse HEAD)" = "$framework_pin"
test -z "$(git -C "$framework" status --porcelain --untracked-files=all)"
test ! -e "$output"
stage=$(mktemp -d /private/tmp/oddlang-cli.XXXXXXXX)
outside=$(mktemp /private/tmp/oddlang-outside.XXXXXXXX)
trap 'rm -rf "$stage"; rm -f "$outside"' EXIT
stage=$(realpath "$stage")
rustc=$(rustup which --toolchain "$toolchain" rustc)
cargo=$(rustup which --toolchain "$toolchain" cargo)
sysroot=$("$rustc" --print sysroot)
registry=$(realpath "${CARGO_HOME:-"$HOME/.cargo"}/registry")
python=$(python3 -c "import os, sys; print(os.path.realpath(sys.executable))")
python_prefix=$("$python" -c "import os, sys; print(os.path.realpath(sys.base_prefix))")
sdk=$(xcrun --sdk macosx --show-sdk-path)
developer_dir=$(xcode-select --print-path)
clang=$(xcrun --sdk macosx --find clang)
archive=$(xcrun --sdk macosx --find ar)
host_var=$("$rustc" -vV | /usr/bin/sed -n 's/^host: //p' | /usr/bin/tr '[:lower:]-' '[:upper:]_')
linker_var="CARGO_TARGET_${host_var}_LINKER"
test "$("$rustc" -vV | /usr/bin/sed -n 's/^commit-hash: //p')" = 8bab26f4f68e0e26f0bb7960be334d5b520ea452
(
    cd "$framework"
    unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC_BOOTSTRAP RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
    RUSTC="$rustc" "$cargo" build --locked --offline --bin oddlang --target-dir "$stage/compiler"
)
oddlang="$stage/compiler/debug/oddlang"
"$python" "$project" recover "$oddlang" "$original"
relative_project=${project#"$companion"/}
test "$relative_project" != "$project"
mkdir -p "$stage/cell" "$stage/home/.cargo" "$stage/tmp"
cp "$oddlang" "$stage/oddlang"
"$python" "$project" stage "$stage/cell"
rm -rf "$stage/compiler"
ln -s "$registry" "$stage/home/.cargo/registry"

quoted_path() {
    case "$1" in
        *'"'*|*'\'*|*$'\n'*) return 1 ;;
        *) printf '%s' "$1" ;;
    esac
}
profile="$stage/profile.sb"
original_objects=$(git -C "$original" rev-parse --path-format=absolute --git-common-dir)
companion_objects=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)
cat > "$profile" <<PROFILE
(version 1)
(allow default)
(deny network*)
(deny file-read-data)
(deny file-write*)
(allow file-write* (subpath "$(quoted_path "$stage")") (literal "/dev/null"))
(allow file-read-data (subpath "$(quoted_path "$stage")") (subpath "$(quoted_path "$sysroot")")
    (subpath "$(quoted_path "$registry")") (subpath "$(quoted_path "$sdk")")
    (subpath "$(quoted_path "$developer_dir")") (subpath "$(quoted_path "$python_prefix")")
    (subpath "/System") (subpath "/usr") (subpath "/bin") (subpath "/sbin")
    (subpath "/opt/homebrew") (subpath "/dev") (subpath "/private/etc")
    (literal "/") (literal "/private") (literal "/tmp")
    (literal "/private/tmp") (literal "/private/var") (literal "/private/var/select/sh"))
(deny file-read-data (subpath "$(quoted_path "$original")")
    (subpath "$(quoted_path "$original_objects")") (subpath "$(quoted_path "$companion")")
    (subpath "$(quoted_path "$companion_objects")") (subpath "$(quoted_path "$framework")"))
PROFILE
probe=("$original/crates/marmish-native/src/cli.rs" "$transcript" "$original_objects/HEAD"
    "$companion_objects/HEAD" "$framework/src/main.rs")
if [[ -n "$oracle" ]]; then
    printf '(deny file-read-data (subpath "%s"))\n' "$(quoted_path "$oracle")" >> "$profile"
    probe+=("$oracle/marmish" "$oracle/marmishd")
fi
(
    cd "$stage"
    sandbox-exec -f "$profile" /bin/sh -c '
    outside=$1
    shift
    if (printf denied > "$outside") 2>/dev/null; then
        echo "generation sandbox can write outside staging" >&2
        exit 1
    fi
    for path do
        if /usr/bin/head -c 1 "$path" >/dev/null 2>&1; then
            echo "generation sandbox can read $path" >&2
            exit 1
        fi
    done
' smoke "$outside" "${probe[@]}"
)
(
    cd "$stage/cell"
    env -i PATH="$(dirname "$cargo"):/usr/bin:/bin" HOME="$stage/home" TMPDIR="$stage/tmp" \
        CARGO_HOME="$stage/home/.cargo" RUSTC="$rustc" CARGO_NET_OFFLINE=true SDKROOT="$sdk" CC="$clang" AR="$archive" "$linker_var=$clang" \
        sandbox-exec -f "$profile" "$python" "$stage/cell/$relative_project" \
        generate "$stage/oddlang" "$stage/workspace"
)
mv "$stage/workspace" "$output"
