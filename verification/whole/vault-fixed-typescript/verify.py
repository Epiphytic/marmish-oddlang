#!/usr/bin/env python3
"""Project and observe the released vault width through native Node TypeScript."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
DATA = ROOT / "oddlang/portable/vault-fixed-bytes-v1"
GRAPH = DATA / "graph.json"
GRAPH_SHA256 = "f87cf4fad9f37189dd02c5a4179e90017380bfbc8affb867eab5fc23a2d6047b"
FRAMEWORK = "95a7314f3699a4fd169a27dd52b424a36b4bea71"
VALUE = "urn:marmish:vault:secret"
FAILURE = "urn:marmish:vault:error:malformed"
OUTPUTS = {"values.mts", "obligations.json", "projection.lock"}


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def project(prefix, compiler, input_file, output, work):
    run(*prefix, compiler, "build", "checked-values", "typescript", input_file, output, cwd=work)
    assert {path.name for path in output.iterdir()} == OUTPUTS
    return {name: (output / name).read_bytes() for name in OUTPUTS}


def observe(prefix, witness, output, width, work):
    runtime = run(*prefix, "node", "--experimental-strip-types", witness,
                  output / "values.mts", VALUE, str(width), FAILURE, cwd=work)
    result = json.loads(runtime)
    assert result == {
        "short": FAILURE, "long": FAILURE, "shadowedWidth": FAILURE, "detached": FAILURE,
        "owned": {"width": width, "inputCopy": True, "outputCopy": True},
        "crossRealm": True,
    }
    return result


def qualify(work, framework, compiler, raw_graph):
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    profile = "(version 1)(allow default)(deny network*)"
    denied = (ROOT, framework, Path("/private/tmp"), Path.home() / "repos")
    assert not any(work.is_relative_to(path) for path in denied)
    profile += "".join("(deny file-read* (subpath " + json.dumps(str(path)) + "))"
                       for path in denied)
    prefix = ["/usr/bin/sandbox-exec", "-p", profile]
    probes = {
        "rustBinding": DATA / "rust.json",
        "rustWitness": HERE.parent / "unified-recovered-vault/fixed_bytes.rs",
    }
    for file in probes.values():
        assert file.is_file()
        result = subprocess.run([*prefix, "/bin/cat", file], capture_output=True)
        assert result.returncode and b"Operation not permitted" in result.stderr, file

    tools = work / "tools"
    inputs = work / "inputs"
    tools.mkdir()
    inputs.mkdir()
    cli = tools / "oddlang"
    shutil.copyfile(compiler, cli)
    cli.chmod(0o755)
    graph = inputs / "domain.json"
    graph.write_bytes(raw_graph)
    assert list(inputs.iterdir()) == [graph]
    baseline = work / "baseline"
    artifacts = project(prefix, cli, graph, baseline, work)

    definition = json.loads(raw_graph)
    graph.write_text(json.dumps(definition, sort_keys=True, separators=(",", ":")))
    assert artifacts == project(prefix, cli, graph, work / "reordered", work)
    definition["types"][VALUE]["length"] = 31
    graph.write_text(json.dumps(definition, indent=2) + "\n")
    mutation = work / "mutation"
    project(prefix, cli, graph, mutation, work)

    witness = tools / "witness.mjs"
    shutil.copyfile(framework / "verification/checked-values/typescript.mjs", witness)
    obligation = json.loads(artifacts["obligations.json"])
    value, = obligation["obligations"]
    assert value["declaration"] == VALUE
    assert value["expected"] == {
        "length": 32, "failure": FAILURE, "input": "exact-bytes", "ownsInputCopy": True,
        "view": "copy", "onDrop": "erase",
    }
    assert value["disposal"]["qualification"] == "unsupported" and value["disposal"]["reason"]
    lock = json.loads(artifacts["projection.lock"])
    assert "bindingSha256" not in lock
    assert lock["sourceSha256"] == sha(artifacts["values.mts"])
    assert lock["obligationsSha256"] == sha(artifacts["obligations.json"])
    native_receipt = HERE.parent / "vault-fixed-bytes/aarch64-apple-darwin.json"
    assert json.loads(native_receipt.read_bytes())["graphSha256"] == GRAPH_SHA256
    return {
        "frameworkCommit": FRAMEWORK,
        "companionRevision": run("git", "rev-parse", "HEAD", cwd=ROOT).decode().strip(),
        "node": run(*prefix, "node", "--version", cwd=work).decode().strip(),
        "platform": "aarch64-apple-darwin",
        "graphSha256": GRAPH_SHA256,
        "nativeGraphQualificationSha256": sha(native_receipt.read_bytes()),
        "compilerSha256": sha(cli.read_bytes()),
        "verifierSha256": sha(Path(__file__).read_bytes()),
        "nodeWitnessSha256": sha(witness.read_bytes()),
        "deniedReadProbes": sorted(probes),
        "inputFiles": ["domain.json"],
        "artifactSha256": {file: sha(artifacts[file]) for file in sorted(artifacts)},
        "reorderedGraphEmitsIdenticalArtifacts": True,
        "baseline": observe(prefix, witness, baseline, 32, work),
        "widthMutation": observe(prefix, witness, mutation, 31, work),
        "disposal": value["disposal"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "compiler_target", "receipt"):
        parser.add_argument(name, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    assert run("git", "rev-parse", "HEAD", cwd=args.framework).decode().strip() == FRAMEWORK
    assert not run("git", "status", "--porcelain", "--untracked-files=no", cwd=args.framework)
    assert not run("git", "status", "--porcelain", "--untracked-files=all", "--",
                   "oddlang/portable", "verification/whole", cwd=ROOT)
    raw_graph = GRAPH.read_bytes()
    assert sha(raw_graph) == GRAPH_SHA256
    run("cargo", "+1.96.1", "build", "--bin", "oddlang", "--offline", "--locked",
        "--target-dir", args.compiler_target, cwd=args.framework)
    parent = run("/usr/bin/getconf", "DARWIN_USER_TEMP_DIR").decode().strip()
    with tempfile.TemporaryDirectory(prefix="odd-vault-typescript-", dir=parent) as temp:
        receipt = qualify(Path(temp).resolve(), args.framework,
                          args.compiler_target / "debug/oddlang", raw_graph)
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
