#!/bin/bash
set -euo pipefail

test "$#" = 3
oddlang=$(realpath "$1")
original=$(realpath "$2")
output=$(realpath "$(dirname "$3")")/$(basename "$3")
here=$(cd "$(dirname "$0")" && pwd -P)
rustup_home=${RUSTUP_HOME:-"$HOME/.rustup"}
cargo_home=${CARGO_HOME:-"$HOME/.cargo"}
rustc_path=$(rustup which rustc)
python=$(command -v python3)
cargo=$(rustup which cargo)
test ! -e "$output"
profile=$(mktemp)
trap 'rm -f "$profile"' EXIT
regex_free_path() {
    case "$1" in
        *'"'*|*'\'*|*$'\n'*) return 1 ;;
        *) printf '%s' "$1" ;;
    esac
}

# Independently observe the original during recovery; keep both its worktree
# and its Git object database unavailable to generation.
git_common=$(git -C "$original" rev-parse --path-format=absolute --git-common-dir)
"$python" "$here/project.py" recover "$oddlang" "$original"
cat > "$profile" <<PROFILE
(version 1)
(allow default)
(deny network*)
(deny file-read* (subpath "$(regex_free_path "$original")"))
(deny file-read* (subpath "$(regex_free_path "$(realpath "$git_common")")"))
(deny file-read* (literal "$(regex_free_path "$here/transcript.json")"))
PROFILE
sandbox-exec -f "$profile" /bin/sh -c '
    if /usr/bin/head -c 1 "$1/crates/marmish-native/src/cli.rs" >/dev/null 2>&1 ||
       /usr/bin/head -c 1 "$2" >/dev/null 2>&1; then
        exit 1
    fi
' smoke "$original" "$here/transcript.json"
env -i PATH="$(dirname "$cargo"):/usr/bin:/bin" HOME="$HOME" RUSTUP_HOME="$rustup_home"     CARGO_HOME="$cargo_home" RUSTC="$rustc_path" CARGO_NET_OFFLINE=true     sandbox-exec -f "$profile" "$python" "$here/project.py" generate "$oddlang" "$output"
"$python" "$here/replay.py" verify "$output/target/release"
