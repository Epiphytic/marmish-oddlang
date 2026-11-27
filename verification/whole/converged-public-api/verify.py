"""Check parent package locks and public behavior after a sealed combined build."""
import argparse
import json
from pathlib import Path
import runpy
import tempfile

CONSTANT = "workspace-v7-recovered-vault-engine-journal-mdk-buffered-limit"
ERRORS = "workspace-v7-engine-public-error-identity"
OUTPUT = "workspace-v7-converged-public-api"

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PORTABLE = ROOT / "oddlang/portable"


def load(name):
    return runpy.run_path(str(HERE.parent / name / "verify.py"))


ENGINE = load("engine-public-error")
LIMIT = load("prototype-constant")
SIX = ENGINE["SIX"]
run, sha, data = (SIX[key] for key in ("run", "sha", "data"))
PARENTS = {name: PORTABLE / name / "workspace.lock" for name in (ERRORS, CONSTANT)}


def check(product):
    current = data(product / "workspace.lock")
    if (product / "workspace.lock").read_bytes() != (PORTABLE / OUTPUT / "workspace.lock").read_bytes():
        raise ValueError("combined workspace differs from its pin")
    for name, path in PARENTS.items():
        parent = data(path)
        if (parent["members"].keys() != current["members"].keys() or
                any(parent[key] != current[key] for key in ("format", "cargoLockSha256", "workspaceCargoManifestSha256"))):
            raise ValueError(f"workspace metadata changed from {name}")
        for member, row in current["members"].items():
            if (member == "marmish-prototype") == (name == CONSTANT) and row != parent["members"][member]:
                raise ValueError(f"{member} differs from qualified {name}")
    SIX["check_member_files"](product, current)
    return current


def observe(args, host, work, scratch, product, build):
    results = ENGINE["observe_types"](args, work, scratch, product, build)
    previous = LIMIT["prior_product"](work / "oddlang", scratch)
    leaf = Path(LIMIT["PRIVATE_LEAF"])
    if sha(previous / leaf) != sha(product / LIMIT["MEMBER"] / leaf):
        raise ValueError("private reader changed in combined workspace")
    results["bufferedInput"] = LIMIT["reader_oracle"](product, args, host, previous)
    original = SIX["artifact"](SIX["cargo_reference"](args, LIMIT["MEMBER"]), "marmish_prototype")
    generated = SIX["artifact"](build, "marmish_prototype")
    results["publicConstant"] = {role: LIMIT["caller"](rlib, scratch, role)
                                 for role, rlib in (("original-constant", original), ("generated-constant", generated))}
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=Path)
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    profile = data(HERE / "import-profile.json")
    args.framework_revision = profile["framework_revision"]
    for checkout, rev in ((args.framework, args.framework_revision), (args.original, SIX["ORIGINAL"]), (ROOT, None)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=checkout):
            raise ValueError(f"dirty checkout: {checkout}")
        if rev and run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() != rev:
            raise ValueError(f"wrong checkout: {checkout}")
    SIX["check_api_release"]("converged", PORTABLE)
    host, = (line[6:] for line in run("rustc", "+1.97.1", "-vV").decode().splitlines() if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError(f"unsupported host: {host}")
    with tempfile.TemporaryDirectory() as w, tempfile.TemporaryDirectory() as s:
        work, scratch = Path(w).resolve(), Path(s).resolve()
        receipt = SIX["qualify"](
            args, host, work, scratch, manifest=PORTABLE / (OUTPUT + ".json"), base=ERRORS + ".json",
            member="marmish-prototype", check=check, extra_denied={
                "original-prototype": args.original / LIMIT["SOURCE"], "constant-caller": LIMIT["FILE"],
                "reader-caller": LIMIT["BUFFERED"]["PRIVATE"] / "observations.rs",
                "original-engine-error": args.original / ENGINE["SOURCE"], "engine-error-caller": ENGINE["HERE"] / "probe.rs",
                "original-native-choice": args.original / ENGINE["IDENTITY"]["SOURCE"],
                "choice-caller": HERE.parent / "backend-choice/probe.rs"},
            extra_artifact_oracle=lambda product, build: observe(args, host, work, scratch, product, build),
            receipt_format="marmish-converged-public-api-v1")
    receipt["parentWorkspaceLocks"] = {name: sha(path) for name, path in PARENTS.items()}
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
