"""Observe the pinned core member and compile an independent typed downstream caller."""

import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from rdflib import Graph, Namespace, RDF

HERE = Path(__file__).resolve().parent
WHOLE = HERE.parent
ROOT = WHOLE.parents[1]
BASE = runpy.run_path(str(WHOLE / "core-lexical/replay.py"))
FRAMEWORK = "ff480c701e02f1d11530303193990d643c1c7da9"
O = Namespace("urn:oddlang:structural-ownership:")
C = Namespace("urn:oddlang:core:")
require = BASE["require"]


def graph_members(member_graph):
    graph = Graph()
    ownership = BASE["PORTABLE"] / "core-structure-v1/ownership.ttl"
    require(BASE["sha"](ownership) == "87b74e908d4f953d517b9c0ea6e1b6065a1f1e5ca60a55ec961d47cb9d1dd2d6",
            "unexpected portable type ownership graph")
    graph.parse(ownership)
    graph += member_graph
    types = set(graph.subjects(RDF.type, O.Type))
    expected = {str(t): [] for t in types}
    declared_members = set(member_graph.subjects(RDF.type, O.Operation)) | set(member_graph.subjects(RDF.type, O.Constant))
    require(set(member_graph.objects(None, O.member)) == declared_members, "unbound or undeclared member")
    for member in declared_members:
        owner, = graph.objects(member, O.owner)
        require(owner in types, "member has an unknown owner")
    paths = {}
    roles = {O.checkedConstruction: "constructor", O.readValue: "getter", O["encode"]: "encode", O["decode"]: "decode"}
    package = BASE["load"](BASE["PACKAGE"])
    inputs = {c["kind"]: c for c in package["components"]}
    for owner in types:
        native, = graph.subjects(O.type, owner)
        paths[str(owner)] = [str(graph.value(native, prop, any=False)).split("::")
                             for prop in (O.originalPath, O.generatedPath)]
        component, = graph.objects(owner, O.semanticComponent)
        component_kind = str(graph.value(component, O.componentKind, any=False))
        overlay = BASE["load"](BASE["PORTABLE"] / inputs[component_kind]["overlay"]["path"])
        for member in graph.subjects(O.owner, owner):
            kind, = graph.objects(member, RDF.type)
            binding, = graph.subjects(O.member, member)
            spelling, = graph.objects(binding, O.spelling)
            if kind == O.Constant:
                prop, = graph.objects(member, O.parameterPredicate)
                if component_kind == "value-domain":
                    require(prop == C.maximum, "wrong range predicate")
                    actual = overlay["maximumConstant"]
                else:
                    actual = overlay["constants"][str(prop).removeprefix(str(C))]
                item_kind = "assoc_const"
            else:
                require(kind == O.Operation, "unknown member kind")
                role, = graph.objects(member, O.role)
                key = "accessor" if role == O.readValue and component_kind == "value-domain" else roles[role]
                actual = overlay[key]
                item_kind = "function"
            require(str(spelling) == actual, "member role differs from the pinned target binding")
            expected[str(owner)].append({"kind": item_kind, "name": actual})
    for items in expected.values():
        items.sort(key=lambda r: (r["kind"], r["name"]))
    return paths, expected


def callers(root, work, host):
    flags = ["cargo", "+1.97.1", "check", "--release", "--offline", "--locked", "--lib", "-p", "marmish-core",
             "--target", host, "--target-dir", work / "target", "--message-format=json"]
    raw = subprocess.check_output(flags, cwd=root, text=True)
    artifacts = [a for a in map(json.loads, raw.splitlines()) if a.get("reason") == "compiler-artifact"
                 and a["target"]["name"] == "marmish_core" and a["target"]["kind"] == ["lib"]]
    artifact, = artifacts
    metadata, = (Path(p) for p in artifact["filenames"] if p.endswith(".rmeta"))
    source = (HERE / "caller.rs").read_text()
    controls = [("complete", source, None),
                ("absent", source.replace("Retention::MAX_SECONDS", "Retention::UNDECLARED"), "E0599"),
                ("wrong-type", source.replace("const _: u64", "const _: u8"), "E0308")]
    results = {}
    for key, variant, code in controls:
        probe = work / "probe.rs"
        probe.write_text(variant)
        p = subprocess.run(["rustc", "+1.97.1", "--edition=2021", "--target", host, "--crate-type=lib",
                            "--emit=metadata", "--error-format=json", "--extern", f"marmish_core={metadata}",
                            "-L", f"dependency={metadata.parent}", "-L", f"dependency={work / 'target/release/deps'}",
                            "-L", f"dependency={work / 'target' / host / 'release/deps'}", probe, "-o", work / "caller.rmeta"], capture_output=True)
        errors = [d["code"]["code"] for d in map(json.loads, p.stderr.splitlines())
                  if d.get("level") == "error" and d.get("code")]
        require((p.returncode == 0 and not errors) if code is None else (p.returncode != 0 and errors == [code]),
                f"unexpected diagnostics for {key}: {p.stderr.decode()}")
        results[key] = errors
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("framework", "original", "candidate", "output"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    args = parser.parse_args()
    BASE["pinned_git"](args.original, BASE["ORIGINAL"])
    seal = BASE["snapshot"](args.candidate)
    host = next(line.removeprefix("host: ") for line in subprocess.check_output(
        ["rustc", "+1.97.1", "-vV"], text=True).splitlines() if line.startswith("host: "))
    member_graph = Graph().parse(BASE["PORTABLE"] / "core-structure-v1/members.ttl")
    paths, documented = graph_members(member_graph)
    changed = Graph() + member_graph
    encoder, = changed.subjects(O.role, O["encode"])
    changed.set((encoder, O.role, O["decode"]))
    try:
        graph_members(changed)
    except ValueError as failure:
        require(str(failure) == "member role differs from the pinned target binding", str(failure))
    else:
        raise ValueError("accepted swapped encode/decode roles")
    receipts = {}
    with BASE["observer_command"](args.framework, FRAMEWORK) as (revision, command):
        for n, (label, root) in enumerate((("original", args.original), ("generated", args.candidate))):
            with tempfile.TemporaryDirectory() as tmp:
                work = Path(tmp)
                report = work / "report.json"
                argv = [*command,
                        root, "marmish-core", "lib", "marmish_core", host, "--public-inherent", "--out", report]
                if label == "original":
                    argv.extend(["--revision", BASE["ORIGINAL"]])
                subprocess.run(argv, check=True)
                data = BASE["load"](report)
                observed = data["rustdocPublicInherent"]
                previous = BASE["load"](WHOLE / "core-lexical" / host / f"{label}.json")
                require({k: v for k, v in data.items() if k != "rustdocPublicInherent"} == previous,
                        "profile or underlying core declarations changed")
                for owner, pair in paths.items():
                    fields = [{"kind": r["kind"], "name": r["name"]} for r in observed["items"] if r["owner"] == pair[n]]
                    require(fields == documented[owner], f"member declarations differ for {owner}")
                receipts[label] = {"completeReportSha256": BASE["sha"](report), "publicInherent": observed,
                                   "caller": callers(root, work, host)}
    require(seal == BASE["snapshot"](args.candidate), "generated input changed")
    BASE["pinned_git"](args.original, BASE["ORIGINAL"])
    BASE["save"](args.output, {"originalRevision": BASE["ORIGINAL"], "observerRevision": revision,
                 "host": host, "inputSeal": seal, "members": documented, "results": receipts,
                 "callerSha256": BASE["sha"](HERE / "caller.rs"),
                 "memberGraphSha256": BASE["sha"](BASE["PORTABLE"] / "core-structure-v1/members.ttl"),
                 "swappedEncodeRoleRejected": True})


if __name__ == "__main__":
    main()
