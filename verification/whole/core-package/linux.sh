#!/usr/bin/env bash
# Native Linux runner. Each invocation gets an empty writable build root.
set -euo pipefail
if [[ $# -ne 4 ]]; then
  echo "usage: linux.sh PORTABLE_DIRECTORY ORIGINAL_CHECKOUT FRAMEWORK_CHECKOUT RECEIPT" >&2
  exit 2
fi
inputs=$(realpath "$1")
original=$(realpath "$2")
framework=$(realpath "$3")
receipt=$(realpath -m "$4")
here=$(cd "$(dirname "$0")" && pwd)
pin=af54339d250290490d62d4475a77539b7e739071
framework_pin=a635eec8e0f71d2dabc9aee28655e26494a7506f
rust_pin=8bab26f4f68e0e26f0bb7960be334d5b520ea452
[[ $(id -u) != 0 ]]
[[ $(git -C "$original" rev-parse HEAD) == "$pin" ]]
[[ $(git -C "$framework" rev-parse HEAD) == "$framework_pin" ]]
[[ -z $(git -C "$original" status --porcelain --untracked-files=all) ]]
[[ -z $(git -C "$framework" status --porcelain --untracked-files=all) ]]
[[ $(rustc +1.97.1 -vV) == *"commit-hash: $rust_pin"* ]]
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/out/tmp" "$work/source" "$work/tests"
(
    cd "$framework"
    unset RUSTC RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC_BOOTSTRAP RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
    cargo +1.97.1 build --locked --offline --bin oddlang --target-dir "$work/framework-target"
)
compiler="$work/framework-target/debug/oddlang"
cargo +1.97.1 build --quiet --locked --offline --manifest-path "$here/../canonical-public-data/Cargo.toml" \
  --bin core-package-inputs --target-dir "$work/qualification-tools"
qualifier="$work/qualification-tools/debug/core-package-inputs"
"$qualifier" stage "$inputs" "$work/in"
toolchain=$(rustc +1.97.1 --print sysroot)
registry=${CARGO_HOME:-$HOME/.cargo}/registry
drop=()
launcher=(bwrap)
privilege_mode=unprivileged-bwrap
if ! bwrap --unshare-all --ro-bind / / /bin/true 2>/dev/null; then
  launcher=(sudo -n bwrap)
  privilege_mode=sudo-bwrap-setpriv
  drop=(setpriv --reuid "$(id -u)" --regid "$(id -g)" --clear-groups --no-new-privs --)
fi
native=("${launcher[@]}" --unshare-net --unshare-pid --unshare-ipc --unshare-uts --die-with-parent --new-session --clearenv
        --ro-bind /usr /usr --symlink usr/bin /bin --symlink usr/lib /lib
        --proc /proc --dev /dev --tmpfs /tmp --perms 0755 --dir /etc --ro-bind /etc/alternatives /etc/alternatives
        --ro-bind "$toolchain" /toolchain --perms 0755 --dir /cargo --ro-bind "$registry" /cargo/registry
        --setenv HOME /tmp --setenv CARGO_HOME /cargo --setenv PATH /toolchain/bin:/usr/bin
        --setenv TMPDIR /out/tmp --setenv LC_ALL C --setenv CARGO_NET_OFFLINE true --bind "$work/out" /out --chdir /out)
if [[ -d /lib64 ]]; then native+=(--ro-bind /lib64 /lib64); fi
local_namespace=$(readlink /proc/self/ns/net)
sandbox_namespace=$("${native[@]}" "${drop[@]}" readlink /proc/self/ns/net)
[[ "$local_namespace" != "$sandbox_namespace" ]]
[[ $("${native[@]}" "${drop[@]}" id -u) == "$(id -u)" ]]
"${native[@]}" "${drop[@]}" grep -Eq '^CapEff:[[:space:]]+0+$' /proc/self/status
"${native[@]}" "${drop[@]}" test ! -e /source
python3 - "${native[@]}" "${drop[@]}" <<'NET'
import socket
import subprocess
import sys

with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    port = listener.getsockname()[1]
    with socket.create_connection(("127.0.0.1", port), timeout=3):
        pass
    code = """
import errno, socket, sys
try:
    socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=3)
except OSError as error:
    if error.errno in (errno.ECONNREFUSED, errno.ENETUNREACH, errno.EHOSTUNREACH, errno.EPERM):
        sys.exit(0)
    raise
raise RuntimeError("isolated process reached host loopback listener")
"""
    subprocess.run([*sys.argv[1:], "python3", "-c", code, str(port)], check=True)
NET
"${native[@]}" --ro-bind "$compiler" /oddlang --ro-bind "$work/in" /inputs \
    "${drop[@]}" /oddlang build native-package rust /inputs/core-package.json /out/generated
cmp "$inputs/core-package-v1/package.lock" "$work/out/generated/package.lock"
"$qualifier" verify "$work/out/generated" "$work/in/core-package.json"
"${native[@]}" --chdir /out/generated "${drop[@]}" cargo build --release --locked --offline
# Source and callers first become available after the clean build succeeds.
git -C "$original" archive "$pin" | tar -x -C "$work/source"
cp "$work/source/crates/marmish-core/tests/contracts.rs" "$work/tests/"
cp "$here/caller.rs" "$work/tests/differential.rs"
mkdir "$work/out/generated/tests"
cp "$work/tests/"*.rs "$work/out/generated/tests/"
cp "$work/tests/differential.rs" "$work/source/crates/marmish-core/tests/"
for impl in generated original; do
  if [[ $impl == generated ]]; then
    runner=("${native[@]}" --chdir /out/generated)
  else
    runner=("${native[@]}" --ro-bind "$work/source" /source --ro-bind "${registry%/registry}/git" /cargo/git --chdir /source)
  fi
  "${runner[@]}" --setenv ODDLANG_CORE_TRANSCRIPT "/out/$impl.jsonl" \
      "${drop[@]}" cargo test --target-dir "/out/tests-$impl" --release --locked --offline -p marmish-core > "$work/$impl.log"
  grep -F '7 passed; 0 failed; 0 ignored' "$work/$impl.log" >/dev/null
  grep -F '1 passed; 0 failed; 0 ignored' "$work/$impl.log" >/dev/null
done
cmp "$work/out/original.jsonl" "$work/out/generated.jsonl"
python3 - "$work" "$receipt" "$pin" "$compiler" "$framework_pin" "$privilege_mode" <<'PY'
import hashlib,json,platform,subprocess,sys
from pathlib import Path
work,receipt=map(Path,sys.argv[1:3])
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
lines=(work/"out/original.jsonl").read_text().splitlines()
assert len(lines)==len({tuple(json.loads(line)[:2]) for line in lines})==1735
receipt.write_text(json.dumps(dict(reference=sys.argv[3],frameworkRevision=sys.argv[5],
    native=platform.machine(),system=platform.system(),privilegeMode=sys.argv[6],effectiveCapabilities=0,
    isolatedHostLoopback=True,verifiedGeneratedFileHashes=True,
    rustc=subprocess.check_output(["rustc","+1.97.1","-vV"],text=True).splitlines(),
    oddlangSha256=sha(Path(sys.argv[4])),packageLockSha256=sha(work/"out/generated/package.lock"),
    graphManifestSha256=sha(work/"in/core-package.json"),callerSha256=sha(work/"tests/differential.rs"),
    contractsSha256=sha(work/"tests/contracts.rs"),publicCases=len(lines),upstreamTestsPerBuild=7,
    transcriptSha256=sha(work/"out/original.jsonl"),nativeGeneration="bubblewrap; network and original source unavailable"),indent=2)+"\n")
PY
