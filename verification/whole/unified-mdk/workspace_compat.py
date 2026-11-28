"""Connect downstream Python qualification callbacks to the Rust workspace verifier."""
from functools import lru_cache
import hashlib
import json
import os
import select
import signal
from pathlib import Path
import runpy
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"
BASE = "workspace-v7-recovered-vault-engine-journal.json"
WORKSPACE = "workspace-v7-recovered-vault-engine-journal-mdk.json"
MEMBER = "marmish-mdk"
MANIFEST = PORTABLE / WORKSPACE
ORIGINAL = "af54339d250290490d62d4475a77539b7e739071"
STANDALONE_MDK = "27b0e5e7b16629cc4ec2973b729cf45e2d9c114b"
PLATFORMS = {"aarch64-apple-darwin", "aarch64-unknown-linux-gnu"}


def lane(name, script="verify.py"):
    return runpy.run_path(str(HERE.parent / name / script))


JOURNAL = lane("journal-records")
run = JOURNAL["run"]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def data(path):
    return json.loads(path.read_bytes())


@lru_cache(maxsize=1)
def workspace_tool():
    target = tempfile.TemporaryDirectory(prefix="odd-workspace-product-")
    run("cargo", "+1.96.1", "build", "--quiet", "--offline", "--locked",
        "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
        "--target-dir", target.name, "--bin", "workspace-product")
    return target, Path(target.name) / "debug/workspace-product"


def product_witness(*argv, current=None):
    _, tool = workspace_tool()
    stdin = json.dumps(current, default=str).encode() if current is not None else None
    timeout = 4200 if argv[0] in ("buffered-oracle", "manifest-oracle", "build-import-libraries", "import-role-report") else 600
    with subprocess.Popen([tool, *argv], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          start_new_session=True, env={**os.environ, "MARMISH_WORKSPACE_CALLBACK_SESSION": "1"}) as process:
        try:
            output, _ = process.communicate(stdin, timeout=timeout)
            if process.returncode:
                raise subprocess.CalledProcessError(process.returncode, process.args)
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise
    return json.loads(output)


def check_api_release(release, portable):
    _, tool = workspace_tool()
    run(tool, "compose-api", release, portable, "--check")


def check_product(product):
    return product_witness("six", product)


def check_member_files(product, current):
    product_witness("members", product, current=current)


def artifact(build, name):
    matches = []
    for line in build.splitlines():
        row = json.loads(line)
        if row["reason"] == "compiler-artifact" and row["target"]["name"] == name:
            matches.extend(Path(value) for value in row["filenames"] if value.endswith(".rlib"))
    if len(matches) != 1:
        raise ValueError(f"ambiguous native artifact {name}: {matches}")
    return matches[0]


def cargo_reference(args, name):
    return run("cargo", "+1.97.1", "build", "--release", "--offline", "--locked",
               "--lib", "-p", name, "--message-format=json", "--target-dir",
               args.original_target, cwd=args.original, env=JOURNAL["cargo_env"]())


def qualify(args, host, work, scratch, *, manifest=MANIFEST, base=BASE, member=MEMBER,
            check=check_product, extra_denied=None, extra_oracle=None, extra_artifact_oracle=None,
            receipt_format="marmish-six-member-mdk-public-data-v1", fixed_vault_predecessor=None):
    """Run the native workspace gate with the downstream release's Python callbacks."""
    request, receipt = scratch / "extension.json", scratch / "extension-receipt.json"
    request.write_text(json.dumps({
        "host": host, "work": work, "scratch": scratch, "manifest": manifest, "base": base,
        "member": member, "format": receipt_format, "fixedVaultPredecessor": fixed_vault_predecessor,
        "extraDenied": extra_denied or {},
    }, default=str))
    target, _ = workspace_tool()
    run("cargo", "+1.96.1", "build", "--quiet", "--offline", "--locked",
        "--manifest-path", HERE.parent / "canonical-public-data/Cargo.toml",
        "--target-dir", target.name, "--bin", "qualify-six-member")
    argv = [Path(target.name) / "debug/qualify-six-member", args.framework, args.original, receipt,
            "--compiler-target", args.compiler_target, "--original-target", args.original_target,
            "--original-bin-dir", args.original_bin_dir, "--framework-revision", args.framework_revision,
            "--extension", request]
    product = work / "product"
    with subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
                          start_new_session=True, env={**os.environ, "MARMISH_WORKSPACE_CALLBACK_SESSION": "1"}) as process:
        try:
            if not select.select([process.stdout], [], [], 1800)[0]:
                raise TimeoutError("native workspace product check timed out")
            if process.stdout.readline().strip() != "check-product":
                raise ValueError("native workspace gate did not reach the sealed product check")
            checked = check(product)
            process.stdin.write(json.dumps(checked) + "\n")
            process.stdin.close()
            if process.wait(timeout=3600):
                raise ValueError("native extended workspace qualification failed")
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise
    result = data(receipt)
    if result["format"] != "marmish-extended-workspace-intermediate-v1" or result.pop("requestedFormat") != receipt_format:
        raise ValueError("unexpected native extension receipt")
    if extra_oracle is not None:
        result.update(extra_oracle(product))
    if extra_artifact_oracle is not None:
        result.update(extra_artifact_oracle(product, (scratch / "workspace-cargo-artifacts.jsonl").read_bytes()))
    result["format"] = receipt_format
    return result
