#!/usr/bin/env bash
# Native Linux runner. Each invocation gets an empty writable build root.
set -euo pipefail
if [[ $# -ne 4 ]]; then
  echo "usage: linux.sh PORTABLE_DIRECTORY ORIGINAL_CHECKOUT ODDLANG RECEIPT" >&2
  exit 2
fi
inputs=$(realpath "$1")
original=$(realpath "$2")
compiler=$(realpath "$3")
receipt=$(realpath -m "$4")
here=$(cd "$(dirname "$0")" && pwd)
pin=af54339d250290490d62d4475a77539b7e739071
[[ $(git -C "$original" rev-parse HEAD) == "$pin" ]]
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/in" "$work/out/tmp" "$work/source" "$work/tests"
python3 - "$inputs" "$work/in" <<'PY'
import hashlib,json,shutil,sys
from pathlib import Path
root,dest=map(Path,sys.argv[1:])
manifest=root/"core-package.json"
data=json.loads(manifest.read_bytes())
for pin in [data["cargoLock"]]+[c[k] for c in data["components"] for k in ("graph","overlay")]:
    src=(root/pin["path"]).resolve()
    assert src.is_relative_to(root) and hashlib.sha256(src.read_bytes()).hexdigest()==pin["sha256"]
    out=dest/src.relative_to(root)
    out.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(src,out)
shutil.copyfile(manifest,dest/manifest.name)
PY
toolchain=$(rustc +1.97.1 --print sysroot)
registry=${CARGO_HOME:-$HOME/.cargo}/registry
drop=()
launcher=(bwrap)
if ! bwrap --unshare-all --ro-bind / / /bin/true 2>/dev/null; then
  launcher=(sudo -n bwrap)
  drop=(setpriv --reuid "$(id -u)" --regid "$(id -g)" --clear-groups --no-new-privs --)
fi
native=("${launcher[@]}" --unshare-net --unshare-pid --unshare-ipc --unshare-uts --die-with-parent --new-session --clearenv
        --ro-bind /usr /usr --symlink usr/bin /bin --symlink usr/lib /lib
        --proc /proc --dev /dev --tmpfs /tmp --perms 0755 --dir /etc --ro-bind /etc/alternatives /etc/alternatives
        --ro-bind "$toolchain" /toolchain --perms 0755 --dir /cargo --ro-bind "$registry" /cargo/registry
        --setenv HOME /tmp --setenv CARGO_HOME /cargo --setenv PATH /toolchain/bin:/usr/bin
        --setenv TMPDIR /out/tmp --setenv LC_ALL C --setenv CARGO_NET_OFFLINE true --bind "$work/out" /out --chdir /out)
if [[ -d /lib64 ]]; then native+=(--ro-bind /lib64 /lib64); fi
"${native[@]}" --ro-bind "$compiler" /oddlang --ro-bind "$work/in" /inputs \
    "${drop[@]}" /oddlang build native-package rust /inputs/core-package.json /out/generated
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
      "${drop[@]}" cargo test --target-dir "/out/tests-$impl" --release --locked --offline -p marmish-core \
      --test contracts --test differential > "$work/$impl.log"
  grep -F '7 passed; 0 failed' "$work/$impl.log" >/dev/null
done
cmp "$work/out/original.jsonl" "$work/out/generated.jsonl"
python3 - "$work" "$receipt" "$pin" "$compiler" <<'PY'
import hashlib,json,platform,subprocess,sys
from pathlib import Path
work,receipt=map(Path,sys.argv[1:3])
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
lines=(work/"out/original.jsonl").read_text().splitlines()
assert len(lines)==len({tuple(json.loads(line)[:2]) for line in lines})==1735
receipt.write_text(json.dumps(dict(reference=sys.argv[3],native=platform.machine(),system=platform.system(),
    rustc=subprocess.check_output(["rustc","+1.97.1","-vV"],text=True).splitlines(),
    oddlangSha256=sha(Path(sys.argv[4])),packageLockSha256=sha(work/"out/generated/package.lock"),
    graphManifestSha256=sha(work/"in/core-package.json"),callerSha256=sha(work/"tests/differential.rs"),
    contractsSha256=sha(work/"tests/contracts.rs"),publicCases=len(lines),upstreamTestsPerBuild=7,
    transcriptSha256=sha(work/"out/original.jsonl"),nativeGeneration="bubblewrap; network and original source unavailable"),indent=2)+"\n")
PY
