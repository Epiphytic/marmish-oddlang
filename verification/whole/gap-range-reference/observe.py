"""Run the paired public data-type probe against the pinned upstream engine library."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
COMPILER = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pinned(root):
    if (subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip() != ORIGINAL or
            subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=root)):
        raise ValueError("expected clean pinned upstream checkout")


def copy_control(flags, work, env):
    source = work / "copy_control.rs"
    source.write_text("fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }")
    result = subprocess.run([*flags, "--error-format=json", "--emit=metadata", source, "-o", work / "control.rmeta"],
                            capture_output=True, cwd=work, env=env)
    errors = [r["code"]["code"] for r in map(json.loads, result.stderr.splitlines())
              if r.get("level") == "error" and r.get("code")]
    if result.returncode == 0 or errors != ["E0277"]:
        raise ValueError(f"unexpected Copy control: {result.stderr.decode()}")
    return errors


def observe(original, target):
    pinned(original)
    if target.is_relative_to(original):
        raise ValueError("use an external Cargo target")
    env = {k: os.environ[k] for k in ("HOME", "PATH", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "SDKROOT") if k in os.environ}
    compiler = subprocess.check_output(["rustc", "+1.97.1", "-vV"], env=env, text=True).splitlines()
    if f"commit-hash: {COMPILER}" not in compiler:
        raise ValueError("unexpected Rust compiler")
    host, = [line[6:] for line in compiler if line.startswith("host: ")]
    result = subprocess.check_output(["cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--lib",
                                      "-p", "marmish-engine", "--target", host, "--target-dir", target,
                                      "--message-format=json"], cwd=original, env=env, text=True)
    rows = list(map(json.loads, result.splitlines()))
    libraries = {}
    for name in ("marmish_engine", "serde_json"):
        row, = [r for r in rows if r["reason"] == "compiler-artifact" and r["target"]["name"] == name
                and r["target"]["kind"] == ["lib"] and any(Path(f).is_relative_to(target / host) and
                f.endswith(".rlib") for f in r["filenames"])]
        if name == "marmish_engine" and Path(row["manifest_path"]).resolve() != original / "crates/marmish-engine/Cargo.toml":
            raise ValueError("unexpected engine package")
        file, = [Path(f) for f in row["filenames"] if f.endswith(".rlib")]
        libraries[name] = file
    artifact_seal = {name: sha(lib) for name, lib in libraries.items()}
    links = sorted({link for r in rows if r["reason"] == "build-script-executed" for link in r["linked_paths"]})
    native = target / host / "release/deps"
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        flags = ["rustc", "+1.97.1", "--edition=2024", "--target", host]
        for name, lib in libraries.items():
            flags.extend(["--extern", f"{name}={lib}"])
        for path in [f"dependency={native}", f"dependency={target / 'release/deps'}", *links]:
            flags.extend(["-L", path])
        subprocess.run([*flags, "--cfg", "with_range", HERE / "probe.rs", "-o", work / "probe"], env=env, cwd=work, check=True)
        trait_control = copy_control(flags, work, env)
        vectors = (HERE / "cases.json").read_bytes()
        results = json.loads(subprocess.check_output([work / "probe"], cwd=work, input=vectors, env=env))
    ids = [r["id"] for r in json.loads(vectors)]
    if len(ids) != len(set(ids)) or [r["id"] for r in results] != ids:
        raise ValueError("incomplete or ambiguous case results")
    pinned(original)
    if artifact_seal != {name: sha(lib) for name, lib in libraries.items()}:
        raise ValueError("selected release library changed while compiling or running the caller")
    return {"original": ORIGINAL, "host": host, "compiler": COMPILER, "cargoLockSha256": sha(original / "Cargo.lock"),
            "modelSha256": sha(original / "crates/marmish-engine/src/model.rs"), "casesSha256": sha(HERE / "cases.json"),
            "probeSha256": sha(HERE / "probe.rs"), "observerSha256": sha(Path(__file__)),
            "copyControl": trait_control, "librarySha256": artifact_seal, "results": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "target", "output"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    args.output.write_text(json.dumps(observe(args.original, args.target), indent=2, sort_keys=True) + "\n")
