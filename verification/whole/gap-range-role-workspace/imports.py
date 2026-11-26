"""Compile external import witnesses against the retained native range build."""

import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess

from verify import AUDIT, BASELINE, WHOLE, data, digest


def check(original, build):
    rustc = subprocess.check_output(["rustc", "+1.97.1", "-vV"], text=True).splitlines()
    host, = [line[6:] for line in rustc if line.startswith("host: ")]
    filename = host + ".json"
    receipts = {name: WHOLE / name / filename for name in
                ("canonical-public-data", "canonical-range-data", "canonical-recovered-range", "gap-range-role-workspace")}
    measured = data(receipts["canonical-range-data"])
    recovered = data(receipts["canonical-recovered-range"])
    role = data(receipts["gap-range-role-workspace"])
    raw = (build / "cargo-artifacts.jsonl").read_bytes()
    product, target = build / "candidate", build / "target"
    if (rustc != measured["rustc"] or hashlib.sha256(raw).hexdigest() != measured["cargoArtifactLogSha256"]
            or digest(product / "workspace.lock") != measured["workspace"]["workspaceLockSha256"]
            or recovered["priorNativeReceiptSha256"] != digest(receipts["canonical-range-data"])
            or role["predecessorReceiptSha256"] != digest(receipts["canonical-recovered-range"])):
        raise ValueError("native library build or successor evidence differs")
    closure = BASELINE["PRIOR"]["library_closure"]
    files = closure(raw, target, host)
    if (len(files) != measured["cargoLibraryClosureEntries"] or
            hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest() != measured["cargoLibraryClosureSha256"]):
        raise ValueError("native Cargo libraries differ from the qualified build")
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=original, text=True).strip() != AUDIT.ORIGINAL:
        raise ValueError("unexpected original workspace revision")
    artifacts, _ = AUDIT.artifacts(raw, product, ["marmish-" + n for n in AUDIT.COUNTS], target, generated=True)
    baseline = data(receipts["canonical-public-data"])["imports"]["libraries"]
    summary = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as jobs:
        pending = {n: jobs.submit(AUDIT.imports, n, artifacts["marmish-" + n], original, target=host)
                   for n in AUDIT.COUNTS}
        for name, job in pending.items():
            result = job.result()
            old = baseline[name]
            for key in ("inventorySha256", "inventoryLockSha256", "inventoryEvidenceSha256"):
                if result[key] != old[key]:
                    raise ValueError("upstream import inventory changed")
            probes = {}
            for before, after in zip(old["imports"], result["imports"], strict=True):
                if (before["spelling"], before["originalInventoryKind"]) != (after["spelling"], after["originalInventoryKind"]):
                    raise ValueError("different import witnesses")
                codes = [d["code"] for d in after["diagnostics"]]
                probes[after["spelling"]] = {"compiled": after["compiles"], "diagnostics": codes}
                if (name, after["spelling"]) == ("engine", "MissingRange"):
                    if before["compiles"] or not after["compiles"]:
                        raise ValueError("expected the recovered range to become importable")
                elif before["compiles"] != after["compiles"] or [d["code"] for d in before["diagnostics"]] != codes:
                    raise ValueError("another import outcome changed")
            summary[name] = {"accepted": result["importable"], "attempted": len(probes), "imports": probes}
    if files != closure(raw, target, host):
        raise ValueError("library closure changed during the compile-only audit")
    return {"host": host, "rustc": rustc, "runnerSha256": digest(Path(__file__)),
            "receiptDigests": {k: digest(v) for k, v in receipts.items()},
            "measuredLibraryWorkspace": measured["workspace"]["workspaceLockSha256"],
            "equivalentSourceWorkspace": role["workspaceLockSha256"], "libraries": summary}


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    for arg in ("original", "retained_build", "receipt"):
        cli.add_argument(arg, type=lambda s: Path(s).resolve())
    args = cli.parse_args()
    args.receipt.write_text(json.dumps(check(args.original, args.retained_build), indent=2) + "\n")
