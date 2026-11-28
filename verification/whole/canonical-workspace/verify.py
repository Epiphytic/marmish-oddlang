"""Exercise all selected contracts on the same sealed native workspace."""
import argparse
import json
from pathlib import Path
import runpy
import sys
import tempfile

BASE = "workspace-v7-converged-diagnostics-cli"
ENGINE = "marmish-engine"
NATIVE = "marmish-native"
OUTPUT = "workspace-v7-canonical-offline"
PREFIX = "workspace-v7-terminal-prefix"
PUBLIC = "workspace-v7-converged-public-api"
RECORDS = "workspace-v7-observational-records"
VAULT = "marmish-vault"
VAULT_MODE = "vault-fixed-value-mode-package.json"

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WHOLE = HERE.parent
PORTABLE = ROOT / "oddlang/portable"
MANIFEST = PORTABLE / (OUTPUT + ".json")
DARWIN = "aarch64-apple-darwin"


def lane(name, program="verify.py"):
    saved = {key: sys.modules.pop(key) for key in ("compose", "verify") if key in sys.modules}
    directory = WHOLE / name
    sys.path.insert(0, str(directory))
    try:
        return runpy.run_path(str(directory / program))
    finally:
        sys.path.pop(0)
        for key in ("compose", "verify"):
            sys.modules.pop(key, None)
        sys.modules.update(saved)


SIX = lane("unified-mdk", "workspace_compat.py")
CLI = lane("terminal-prefix")
QUALIFICATION = lane("core-package")
VAULT_SOURCE = "crates/marmish-vault/src/helper.rs"
run, data, sha = (SIX[name] for name in ("run", "data", "sha"))


def check(product):
    return SIX["product_witness"]("canonical-offline-product", product)


def terminal(args, host, work, scratch, product, build):
    captured = data(WHOLE / "terminal-prefix" / f"{host}.json")
    if captured["original"] != SIX["ORIGINAL"] or captured["workspaceLockSha256"] != sha(PORTABLE / PREFIX / "workspace.lock"):
        raise ValueError("terminal corpus needs its matching native reference receipt")
    roles = CLI["ROLES"]
    binaries = CLI["PRIOR"]["binaries"](build, product, product / "target")
    if binaries.keys() != set(roles):
        raise ValueError("missing canonical CLI role")
    launcher = None
    if host != DARWIN:
        isolation, _ = CLI["WORKSPACE"]["linux_isolation"](work)
        launcher = isolation[:isolation.index("--unshare-net")]
    outcome = {}
    for role in roles:
        orig = args.original_bin_dir / role
        original_witness = captured["roles"][role]
        if sha(orig) != original_witness["originalSha256"]:
            raise ValueError(f"wrong original release executable: {role}")
        cases = CLI["corpus"](role)
        if cases != [row["args"] for row in original_witness["cases"]]:
            raise ValueError(f"unexpected terminal argument corpus: {role}")
        home = scratch / f"terminal-{role}"
        home.mkdir()
        for argv, row in zip(cases, original_witness["cases"]):
            reference = {key: value for key, value in row.items() if key != "args"}
            original = CLI["observe"](orig, argv, home, launcher)
            generated = CLI["observe"](binaries[role], argv, home, launcher)
            if original != reference or generated != reference or original["exit"] not in (0, 255):
                raise ValueError(f"canonical {role} terminal output differs for {argv!r}")
        control = original_witness["outsideTerminalProfile"]
        argv = CLI["POST_PARSE"]
        original = CLI["observe"](orig, argv, home, launcher)
        generated = CLI["observe"](binaries[role], argv, home, launcher)
        if control != {"args": argv, "original": original, "candidate": generated} or any(home.iterdir()):
            raise ValueError(f"terminal case isolation or negative outcome differs for {role}")
        outcome[role] = {"cases": len(cases), "generatedSha256": sha(binaries[role]),
                         "originalSha256": sha(orig), "outsideTerminalProfile": control}
    return {"terminalCli": {"roles": outcome, "witnessSha256": sha(WHOLE / "terminal-prefix" / f"{host}.json")}}


def diagnostics(args, host, work, scratch, product, build):
    root = scratch / "diagnostics"
    root.mkdir()
    archived = WHOLE / "public-diagnostics" / f"{host}.json"
    expected = data(archived)
    source = args.original / "crates/marmish-native/src/diagnostics.rs"
    probe = WHOLE / "public-diagnostics/probe.rs"
    if expected["original"] != SIX["ORIGINAL"] or expected["sourceSha256"] != sha(source) or expected["probeSha256"] != sha(probe):
        raise ValueError("unexpected original diagnostics pin")
    recovered = root / "graph.json"
    run(work / "oddlang", "recover", "structured-diagnostics", source, recovered)
    if recovered.read_bytes() != (work / "inputs/native-diagnostics-v1/graph.json").read_bytes():
        raise ValueError("native diagnostic source observations drifted")
    lib = SIX["artifact"](build, "marmish_native")
    actual = SIX["product_witness"]("diagnostic-caller", lib, host, "0.3.1", root, product / "target/release/deps")
    if actual != expected["observations"]:
        raise ValueError("canonical native diagnostic observations differ")
    return {"diagnostics": {"observations": actual, "originalWitnessSha256": sha(archived),
                            "sourceGraphSha256": sha(recovered), "generatedLibrarySha256": sha(lib)}}


def vault_mode(args, host, work, scratch, product, build):
    root = scratch / "vault-mode"
    root.mkdir()
    archived = WHOLE / "vault-mode" / f"{host}.json"
    expected = data(archived)
    if expected["original"] != SIX["ORIGINAL"] or expected["probeSha256"] != sha(WHOLE / "vault-mode/probe.rs"):
        raise ValueError("unexpected original vault mode pin")
    generated = SIX["artifact"](build, "marmish_vault")
    _, observer = QUALIFICATION["qualification_tool"]("vault-mode-observer")
    actual = json.loads(run(observer, generated, root, product / "target/release/deps"))
    expected_result = {field: expected["results"]["baseline"][field] for field in ("values", "absentTraits")}
    if actual != expected_result:
        raise ValueError("canonical vault helper differs from paired original observations")
    source = args.original / VAULT_SOURCE
    recovered = root / "recovered"
    run(work / "oddlang", "recover", "plain-choice", source, WHOLE / "vault-mode/selection.json", recovered)
    for name in ("graph.json", "rust.json"):
        if (recovered / name).read_bytes() != (work / "inputs/vault-mode-v1" / name).read_bytes():
            raise ValueError("canonical vault mode source recovery differs from sealed inputs")
    return {"vaultMode": {"observations": actual, "standaloneWitnessSha256": sha(archived),
                          "generatedLibrarySha256": sha(generated), "sourceSha256": sha(source)}}


def imports(args, product, build):
    return SIX["product_witness"]("inherited-imports", "canonical", product, product / "target",
                                  args.original, current=build.decode())


def denied(args, host):
    return {
        "original-prototype": args.original / "crates/marmish-prototype/src/lib.rs",
        "constant-caller": WHOLE / "prototype-constant/caller.rs",
        "reader-caller": HERE.parent / "private-buffer/observations.rs",
        "original-engine-error": args.original / "crates/marmish-engine/src/model.rs",
        "engine-error-caller": WHOLE / "engine-public-error/probe.rs",
        "original-native-choice": args.original / "crates/marmish-native/src/identity.rs",
        "choice-caller": WHOLE / "backend-choice/probe.rs",
        "original-release-manifest": args.original / "crates/marmish-native/src/native_manifest.rs",
        "original-release-versions": args.original / "tools/release-version-vectors.json",
        "manifest-oracle-caller": WHOLE / "release-manifest/probe.rs",
        "manifest-blind-vectors": WHOLE / "release-manifest/vectors.json",
        "manifest-original-receipt": WHOLE / "release-manifest" / f"{host}.json",
        "original-engine-records": args.original / "crates/marmish-engine/src/model.rs",
        "original-native-path-records": args.original / "crates/marmish-native/src/transport.rs",
        "engine-record-caller": WHOLE / "engine-native-records/engine.rs",
        "native-record-caller": WHOLE / "engine-native-records/native.rs",
        "original-native-diagnostics": args.original / "crates/marmish-native/src/diagnostics.rs",
        "diagnostic-caller": WHOLE / "public-diagnostics/probe.rs",
        "original-vault-mode": args.original / VAULT_SOURCE,
        "vault-mode-caller": WHOLE / "vault-mode/probe.rs",
        "terminal-original-witness": WHOLE / "terminal-prefix" / f"{host}.json",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "receipt"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    for name in ("compiler-target", "original-target", "original-bin-dir"):
        parser.add_argument("--" + name, type=lambda path: Path(path).resolve(), required=True)
    parser.add_argument("--framework-revision", required=True)
    args = parser.parse_args()
    for checkout, revision in ((ROOT, None), (args.original, SIX["ORIGINAL"]), (args.framework, args.framework_revision)):
        if run("git", "status", "--porcelain", "--untracked-files=all", cwd=checkout) or (
            revision and run("git", "rev-parse", "HEAD", cwd=checkout).decode().strip() != revision
        ):
            raise ValueError(f"expected clean pinned checkout: {checkout}")
    host, = (line[6:] for line in run("rustc", "+1.97.1", "-vV").decode().splitlines() if line.startswith("host: "))
    if host not in SIX["PLATFORMS"]:
        raise ValueError("expected one of the qualified native ARM64 hosts")
    SIX["check_api_release"]("canonical-offline", PORTABLE)
    old_vault = data(PORTABLE / (BASE + ".json"))["members"][VAULT]["manifest"]
    before = PORTABLE / old_vault["path"]
    if sha(before) != old_vault["sha256"]:
        raise ValueError("unexpected fixed-value vault predecessor")
    with tempfile.TemporaryDirectory(prefix="odd-canonical-build-") as build_dir, tempfile.TemporaryDirectory(
        prefix="odd-canonical-checks-"
    ) as test_dir:
        work, scratch = Path(build_dir).resolve(), Path(test_dir).resolve()

        def observe(product, build):
            result = {} if host != DARWIN else imports(args, product, build)
            result.update(terminal(args, host, work, scratch, product, build))
            result.update(diagnostics(args, host, work, scratch, product, build))
            result.update(vault_mode(args, host, work, scratch, product, build))
            for selection in ("--records-work", "--inherited-work"):
                result.update(SIX["product_witness"](
                    "manifest-oracle", product, args.original, args.original_target, scratch, host,
                    scratch / "workspace-cargo-artifacts.jsonl", selection, work))
            result["releaseManifest"] = SIX["product_witness"](
                "manifest-oracle", product, args.original, args.original_target, scratch, host,
                scratch / "workspace-cargo-artifacts.jsonl")
            check(product)
            return result

        receipt = SIX["qualify"](
            args, host, work, scratch, manifest=MANIFEST, base=BASE + ".json", member=NATIVE, check=check,
            extra_artifact_oracle=observe, extra_denied=denied(args, host),
            receipt_format="marmish-canonical-offline-workspace-v1",
            fixed_vault_predecessor=before,
        )
    receipt["unchangedMemberLocks"] = [member for member in receipt["physicalMembers"]
                                       if member not in {NATIVE, ENGINE, VAULT}]
    receipt["predecessors"] = {name: sha(PORTABLE / name / "workspace.lock")
                                for name in (BASE, PREFIX, RECORDS, PUBLIC)}
    receipt["vaultModePackageSha256"] = sha(PORTABLE / VAULT_MODE)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
