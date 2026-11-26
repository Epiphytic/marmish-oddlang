"""Project the reason choice from sealed inputs, then run the same native wire probe."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from observe import COMPILER, HERE, sha

PORTABLE = HERE.parents[2] / "oddlang/portable"
SPEC = PORTABLE / "engine-gap-reason-package.json"
REFERENCE = PORTABLE / "engine-gap-reason-v1/package.lock"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def project(compiler):
    host_info = subprocess.check_output(["rustc", "+1.97.1", "-vV"], text=True).splitlines()
    require(f"commit-hash: {COMPILER}" in host_info, "unexpected Rust compiler")
    host, = [v[6:] for v in host_info if v.startswith("host: ")]
    original_file = HERE / f"original-{host}.json"
    original = json.loads(original_file.read_bytes())
    require(original["casesSha256"] == sha(HERE / "cases.json") and original["probeSha256"] == sha(HERE / "probe.rs"),
            "reference wire probe changed")
    cases = [case for case in json.loads((HERE / "cases.json").read_bytes()) if case["type"] == "reason"]
    case_ids = {row["id"] for row in cases}
    expected = [row for row in original["results"] if row["id"] in case_ids]
    env = {k: os.environ[k] for k in ("HOME", "PATH", "CARGO_HOME", "TMPDIR", "RUSTUP_HOME", "SDKROOT") if k in os.environ}
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        spec = json.loads(SPEC.read_bytes())
        staged = work / "inputs"
        staged.mkdir()
        for pin in [spec["cargoLock"], *(c[k] for c in spec["components"] for k in ("graph", "overlay"))]:
            file = PORTABLE / pin["path"]
            require(sha(file) == pin["sha256"], "component graph, overlay or Cargo lock changed")
            dst = staged / pin["path"]
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(file, dst)
        shutil.copy(SPEC, staged / SPEC.name)
        product = work / "product"
        subprocess.run([compiler, "build", "native-package", "rust", staged / SPEC.name, product],
                       cwd=work, env=env, check=True)
        require((product / "package.lock").read_bytes() == REFERENCE.read_bytes(), "generated package lock changed")
        lock = json.loads(REFERENCE.read_bytes())
        require(all(sha(product / path) == digest for path, digest in lock["generatedFiles"].items()),
                "generated files differ from their lock")
        caller = work / "caller"
        (caller / "src").mkdir(parents=True)
        shutil.copy(HERE / "caller.toml", caller / "Cargo.toml")
        shutil.copy(HERE / "caller.lock", caller / "Cargo.lock")
        shutil.copy(HERE / "probe.rs", caller / "src/main.rs")
        flags = ["cargo", "+1.97.1", "--offline", "--locked", "--release", "--target", host]
        command = [flags[0], flags[1], "run", *flags[2:], "--quiet"]
        observations = json.loads(subprocess.check_output(command, input=json.dumps(cases).encode(), cwd=caller, env=env))
        require(observations == expected and len(observations) == len(cases), "candidate wire outcomes differ")
        (caller / "src/main.rs").write_text("fn need<T: Copy>() {} fn main() { need::<marmish_engine::MissingReason>(); }")
        reject = subprocess.run([flags[0], flags[1], "check", *flags[2:], "--message-format=json"],
                                cwd=caller, env=env, capture_output=True)
        errors = [r["message"]["code"]["code"] for r in map(json.loads, reject.stdout.splitlines())
                  if r.get("reason") == "compiler-message" and r["message"]["level"] == "error" and r["message"].get("code")]
        require(reject.returncode and errors == original["copyControl"] == ["E0277"], "wrong Copy diagnostic")
    return {"host": host, "frameworkBinarySha256": sha(compiler), "manifestSha256": sha(SPEC),
            "packageLockSha256": sha(REFERENCE), "referenceSha256": sha(original_file),
            "verifierSha256": sha(Path(__file__)), "copyControl": errors, "observations": observations}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("compiler", type=lambda v: Path(v).resolve())
    cli.add_argument("output", type=Path)
    args = cli.parse_args()
    args.output.write_text(json.dumps(project(args.compiler), indent=2, sort_keys=True) + "\n")
