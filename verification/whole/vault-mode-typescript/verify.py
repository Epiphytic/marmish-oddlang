"""Compile and observe the published ordinal graph as TypeScript and Node data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
DOMAIN = Path("oddlang/portable/vault-mode-v1")
PUBLISHED = "154ab6d9a085eac755acd197b2a55e53158a5999"
NATIVE_WITNESS = "622f2a23f3549a902d4bd96d4214ffe05330991b"
HOSTS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")
FRAMEWORK = "86b23552d02f67ee7156a4ce0ece3e07c1fc3fb0"
TSC = ["npx", "--offline", "--package=typescript@5.9.3", "tsc"]
NODE = "v22.22.2"
RUSTC = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"

PROBE = '''
import { AuthorizationMode as Mode } from "./values";
const ordinals = [Mode.ordinal(Mode.Interactive), Mode.ordinal(Mode.Verify)];
const recovered = ordinals.map(value => Mode.ordinal(Mode.fromOrdinal(value)!));
if (!Object.is(ordinals[0], 0) || !Object.is(ordinals[1], 1) || !Object.is(Mode.fromOrdinal(-0), 0))
  throw new Error("exact ordinal bits differ");
const hostStrictEquality = {
  sameOrdinal: Mode.Interactive === Mode.Interactive,
  distinctOrdinals: Mode.Interactive === Mode.Verify,
};
for (const value of [2, -1, "0", null, NaN, Infinity, 0.5]) {
  if (Mode.fromOrdinal(value) !== undefined) throw new Error("invalid ordinal became a mode");
}
if ("equals" in Mode || "partialEquals" in Mode || !Object.isFrozen(Mode))
  throw new Error("mode capability or immutability changed");
console.log(JSON.stringify({ordinals, recovered, equality: Mode.equality, hostStrictEquality}));
'''
NEGATIVE = '''
import { AuthorizationMode as Mode } from "./values";
// @ts-expect-error Ordinal numbers require the checked conversion.
const raw: Mode = 0;
// @ts-expect-error The published choice has no comparison capability.
Mode.equals(Mode.Interactive, Mode.Verify);
// @ts-expect-error The published choice does not declare partial equality.
Mode.partialEquals(Mode.Interactive, Mode.Verify);
'''


def run(*args, cwd=None):
    result = subprocess.run(args, cwd=cwd, capture_output=True, timeout=300)
    if result.returncode:
        raise ValueError((result.stdout + result.stderr).decode(errors="replace"))
    return result.stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def published(path, revision=PUBLISHED):
    return run("git", "-C", ROOT, "show", f"{revision}:{path.as_posix()}")


def verify(framework, target):
    if (run("git", "-C", framework, "rev-parse", "HEAD").decode().strip() != FRAMEWORK
            or run("git", "-C", framework, "status", "--porcelain", "--untracked-files=all")):
        raise ValueError("expected a clean exact compiler checkout")
    if target.is_relative_to(framework) or target.is_relative_to(ROOT):
        raise ValueError("expected an external compiler target")
    if f"commit-hash: {RUSTC}" not in run("rustc", "+1.97.1", "-vV").decode().splitlines():
        raise ValueError("unexpected rustc build tool")
    version = run("node", "--version").decode().strip()
    typescript = run(*TSC, "--version").decode().strip()
    if version != NODE or typescript != "Version 5.9.3":
        raise ValueError("unexpected native JavaScript verification toolchain")
    run("cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--bin", "oddlang",
        "--target-dir", target, cwd=framework)
    compiler = target / "release/oddlang"
    graph = published(DOMAIN / "graph.json")
    obligations = published(DOMAIN / "obligations.json")
    witnesses = {}
    ordinals = []
    for host in HOSTS:
        native = published(Path(f"verification/whole/vault-mode/{host}.json"), NATIVE_WITNESS)
        receipt = json.loads(native)
        reference = receipt["results"]["baseline"]
        if receipt["host"] != host or reference["graphSha256"] != sha(graph):
            raise ValueError("published graph and independent ordinal witness differ")
        values = dict(line.split("=") for line in reference["values"])
        if set(values) != {"interactive", "verify"}:
            raise ValueError("published witness describes different cases")
        if reference["absentTraits"]["Eq"] != "E0277" or reference["absentTraits"]["PartialEq"] != "E0277":
            raise ValueError("native equality witness changes the declared capability")
        witnesses[host] = sha(native)
        ordinals.append([int(values[label]) for label in ("interactive", "verify")])
    values, other = ordinals
    if values != other:
        raise ValueError("independent native ordinals differ by host")
    binding = (ROOT / DOMAIN / "typescript.json").read_bytes()

    with tempfile.TemporaryDirectory(prefix="odd-vault-mode-ts-") as temp:
        scratch = Path(temp)
        inputs = scratch / "inputs"
        inputs.mkdir()
        (inputs / "graph.json").write_bytes(graph)
        (inputs / "typescript.json").write_bytes(binding)
        out = scratch / "output"
        run(compiler, "build", "checked-values", "typescript", "graph.json", "typescript.json", out, cwd=inputs)
        source = (out / "values.ts").read_bytes()
        if (out / "obligations.json").read_bytes() != obligations:
            raise ValueError("language-neutral obligations differ")
        lock = json.loads((out / "projection.lock").read_bytes())
        if lock["sourceSha256"] != sha(source) or lock["obligationsSha256"] != sha(obligations):
            raise ValueError("projection lock does not identify its output")
        (out / "probe.ts").write_text(PROBE)
        (out / "negative.ts").write_text(NEGATIVE)
        flags = ["--strict", "--target", "ES2022", "--module", "commonjs"]
        run(*TSC, *flags, "--noEmit", "negative.ts", cwd=out)
        run(*TSC, *flags, "--declaration", "--outDir", "compiled", "probe.ts", cwd=out)
        result = json.loads(run("node", out / "compiled/probe.js"))
        if result != {"ordinals": values, "recovered": values, "equality": "none",
                      "hostStrictEquality": {"sameOrdinal": True, "distinctOrdinals": False}}:
            raise ValueError("Node choice differs from the published native witness")
        return {
            "publication": PUBLISHED, "framework": FRAMEWORK, "compilerSha256": sha(compiler.read_bytes()),
            "node": version, "typescript": typescript, "graphSha256": sha(graph),
            "verifierSha256": sha(Path(__file__).read_bytes()),
            "nativeWitnessRevision": NATIVE_WITNESS, "nativeWitnessSha256": witnesses,
            "typescriptBindingSha256": sha(binding), "projection": lock,
            "declarationSha256": sha((out / "compiled/values.d.ts").read_bytes()), "observations": result,
            "staticRejections": ["unchecked ordinal", "total equality", "partial equality"],
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    receipt = verify(args.framework, args.compiler_target)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
