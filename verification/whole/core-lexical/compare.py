"""Compare archived lexical declarations and separately audited public import roles."""

import json
from pathlib import Path
import sys

from replay import (
    CORE,
    OBSERVER,
    ORIGINAL,
    PACKAGE,
    PROJECTION,
    ROOT,
    WORKSPACE_LOCK,
    WORKSPACE_SHA,
    load,
    require,
    sha,
)

HERE = Path(__file__).resolve().parent
CANONICAL = ROOT / "verification/whole/canonical-workspace/aarch64-apple-darwin.json"
INVENTORY = ROOT / "verification/whole/public-types/original-core/exports.json"
HOSTS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu")


def declarations(report):
    return {
        (row["kind"], tuple(row["path"])): row
        for row in report["comparison"]["matched"]
    }


def one(rows):
    require(len(rows) == 1, "expected one unambiguous declaration or export")
    return rows[0]


def reports(host):
    directory = HERE / host
    receipt = load(directory / "receipt.json")
    require(
        receipt["host"] == host
        and receipt["before"] == receipt["after"]
        and receipt["before"]["workspaceLockSha256"] == WORKSPACE_SHA,
        "receipt host or provenance mismatch",
    )
    require(
        receipt["observerRevision"] == OBSERVER
        and receipt["originalRevision"] == ORIGINAL
        and receipt["projectionRevision"] == PROJECTION,
        "receipt revision mismatch",
    )
    require(
        receipt["mutation"]
        == {"file": "src/lib.rs", "outcome": "generated file changed: src/lib.rs"},
        "missing changed-input rejection",
    )
    member = load(WORKSPACE_LOCK)["members"][CORE]
    lock_file = HERE / "package.lock"
    lock = load(lock_file)
    require(
        sha(lock_file) == member["standalonePackageLockSha256"]
        and receipt["before"]["coreManifestSha256"]
        == member["inputSha256"]
        == lock["manifestSha256"],
        "wrong standalone package lock or input",
    )
    require(
        receipt["before"]["generatedFiles"]
        == {
            "package.lock": sha(lock_file),
            "Cargo.toml": lock["cargoManifestSha256"],
            "Cargo.lock": lock["cargoLockSha256"],
            **lock["generatedFiles"],
        },
        "receipt inventory disagrees with the sealed package",
    )
    result = []
    for role in ("original", "generated"):
        path = directory / f"{role}.json"
        require(
            sha(path) == receipt["reportsSha256"][role],
            "archived report digest changed",
        )
        report = load(path)
        require(
            report["tools"]["host"] == host
            and report["profile"]["target"] == [host]
            and report["package"] == CORE
            and report["targetKind"] == "lib"
            and report["cargoProfile"] == "release"
            and report["defaultFeatures"]
            and not report["requestedFeatures"],
            "observer was not a native release build",
        )
        if role == "original":
            require(report["source"]["revision"] == ORIGINAL, "wrong original revision")
        else:
            generated = receipt["before"]["generatedFiles"]
            require(
                report["source"] is None
                and report["workspaceManifestSha256"] == generated["Cargo.toml"]
                and report["cargoLockSha256"] == generated["Cargo.lock"],
                "wrong generated inputs",
            )
        result.append(report)
    return result


def path_summary(report):
    data = report["comparison"]
    visibility = data["declaredAccess"]
    return {
        "unambiguous": len(data["matched"]),
        "ambiguous": len(data["ambiguous"]),
        "compilerOnly": len(data["compilerOnly"]),
        "rustdocOnly": len(data["rustdocOnly"]),
        "visibilityAgreements": len(visibility["agreement"]),
        "visibilityDisagreements": len(visibility["different"]),
        "visibilityUnresolved": len(visibility["unresolved"]),
        "outsideVisibilityScopeByKind": visibility["outsideScopeMatchedByKind"],
    }


def compare():
    darwin = reports(HOSTS[0])
    linux = reports(HOSTS[1])
    require(
        [r["comparison"] for r in darwin] == [r["comparison"] for r in linux],
        "different native declaration evidence across hosts",
    )
    original, generated = map(declarations, darwin)
    canonical = load(CANONICAL)
    require(
        canonical["workspaceLockSha256"] == WORKSPACE_SHA,
        "wrong downstream audit workspace",
    )
    imports = canonical["darwinPublicImports"]["libraries"]["core"]
    require(
        imports["inventorySha256"] == sha(INVENTORY), "wrong original export inventory"
    )
    inventory = load(INVENTORY)
    bindings = inventory["modules"][inventory["root"]]["bindings"]
    require(
        {row["spelling"] for row in imports["imports"]} == set(bindings)
        and len(bindings) == 6
        and imports["importable"] == 6
        and imports["missing"] == 0,
        "incomplete Darwin import audit",
    )
    rows = []
    components = load(PACKAGE)["components"]
    package_lock = load(HERE / "package.lock")
    for observation in imports["imports"]:
        name = observation["spelling"]
        require(
            observation["compiles"]
            and observation["originalInventoryKind"]
            == bindings[name]["kind"]
            == "data",
            f"missing public data import {name}",
        )
        position, component = one(
            [(i, c) for i, c in enumerate(components) if name in c["reexports"]]
        )
        sealed_component = package_lock["components"][position]
        require(
            sealed_component["kind"] == component["kind"]
            and sealed_component["module"] == component["module"]
            and sealed_component["reexports"] == component["reexports"]
            and sealed_component["graphSha256"] == component["graph"]["sha256"]
            and sealed_component["overlaySha256"] == component["overlay"]["sha256"],
            "component pins or public bindings disagree with package lock",
        )
        product_file = one(
            [
                file
                for file, digest in package_lock["generatedFiles"].items()
                if digest == sealed_component["sourceSha256"]
            ]
        )
        kind, original_path = one(
            [
                key
                for key in original
                if key[1] == ("marmish_core", component["module"], name)
            ]
        )
        original_export_target = (
            bindings[name]["target"].removeprefix("local::").replace("::{type}", "")
        )
        require(
            "::".join(original_path) == original_export_target,
            "original binding has another owner",
        )
        generated_key = one(
            [
                key
                for key in generated
                if key[0] == kind
                and key[1][:2] == original_path[:2]
                and key[1][-1] == name
            ]
        )
        require(
            original_path != generated_key[1],
            "expected different native declaration paths",
        )
        require(
            all(
                one(generated[generated_key][observer])["file"] == product_file
                for observer in ("compiler", "rustdoc")
            ),
            "generated type is outside its sealed component",
        )
        for key, report in zip(((kind, original_path), generated_key), darwin):
            require(
                any(
                    (r["kind"], tuple(r["path"])) == key
                    and r["compiler"] == r["rustdoc"] == "public"
                    for r in report["comparison"]["declaredAccess"]["agreement"]
                ),
                "unexpected declared visibility",
            )
        rows.append(
            {
                "spelling": name,
                "nativeRole": "crate-root public data reexport",
                "normalizedKind": kind,
                "originalDeclarationPath": original_path,
                "generatedDeclarationPath": generated_key[1],
                "generatedSourceFile": product_file,
                "declaredAccessInBoth": "public",
                "component": {
                    k: component[k] for k in ("kind", "module", "graph", "overlay")
                },
            }
        )
    same = [
        {"kind": kind, "path": path}
        for kind, path in sorted(original.keys() & generated.keys())
    ]
    return {
        "scope": "lexical declarations at a selected native compiler profile",
        "observedHosts": HOSTS,
        "original": path_summary(darwin[0]),
        "generated": path_summary(darwin[1]),
        "sameLexicalKeys": same,
        "publicDataRolePairs": rows,
        "darwinDownstreamImportAudit": {
            "canonicalReceiptSha256": sha(CANONICAL),
            "inventorySha256": sha(INVENTORY),
            "compiled": 6,
            "attempted": 6,
        },
    }


if __name__ == "__main__":
    sys.stdout.write(json.dumps(compare(), indent=2, sort_keys=True) + "\n")
