"""Verify the portable ownership query against pinned native evidence."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys

from rdflib import Graph, Literal, Namespace, RDF, URIRef

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LEXICAL = ROOT / "verification/whole/core-lexical"
sys.path.insert(0, str(LEXICAL))
from compare import compare  # noqa: E402

O = Namespace("urn:oddlang:structural-ownership:")
M = Namespace("urn:marmish:structure:core:")
F = Namespace("urn:oddlang:facade:")
B = Namespace("urn:marmish:binding:rust:core:")
MEMBER = M["canonical-member"]
BASE = "036349fc69ab5b850df2006c16ec2518be95a254"
EVIDENCE = "10926d822bb9aa3a16b20ef35cfc63bebd586780"
MERGE = "2c45e8375690c9ff8f25023bf000ced3b16a6762"
SLUGS = {
    "ConfigError": "configuration-failure",
    "FrameError": "framing-failure",
    "Limits": "frame-limits",
    "ProbeEnvelope": "probe-envelope",
    "RelaySet": "relay-set",
    "Retention": "retention-policy",
}
MODULES = {"config": M.configuration, "frame": M["probe-framing"]}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(revision, name):
    relative = name.relative_to(ROOT).as_posix()
    raw = subprocess.check_output(["git", "show", f"{revision}:{relative}"], cwd=ROOT)
    require(raw == name.read_bytes(), f"pinned evidence changed: {relative}")
    return URIRef(
        f"https://github.com/Epiphytic/marmish-oddlang/blob/{revision}/{relative}"
    ), raw


def verify(graph):
    package = ROOT / "oddlang/portable/core-package.json"
    lock = ROOT / "oddlang/portable/workspace-v7-canonical-offline/workspace.lock"
    facade = ROOT / "oddlang/facade"
    pinned = (
        (O.workspaceLock, O.workspaceLockSha256, BASE, lock),
        (O.packageManifest, O.packageManifestSha256, BASE, package),
        (O.standaloneLock, O.standaloneLockSha256, EVIDENCE, LEXICAL / "package.lock"),
        (O.comparison, None, EVIDENCE, LEXICAL / "comparison.json"),
        (
            O.darwinReceipt,
            None,
            EVIDENCE,
            LEXICAL / "aarch64-apple-darwin/receipt.json",
        ),
        (
            O.linuxReceipt,
            None,
            EVIDENCE,
            LEXICAL / "aarch64-unknown-linux-gnu/receipt.json",
        ),
        (O.originalFacadeGraph, None, MERGE, facade / "domain.ttl"),
        (O.originalFacadeLock, None, MERGE, facade / "import.lock"),
    )
    for predicate, digest, revision, name in pinned:
        uri, raw = pin(revision, name)
        require(
            set(graph.objects(MEMBER, predicate)) == {uri},
            f"wrong evidence link for {name.name}",
        )
        if digest:
            require(
                set(graph.objects(MEMBER, digest))
                == {Literal(hashlib.sha256(raw).hexdigest())},
                f"wrong digest for {name.name}",
            )

    facade_graph = Graph().parse(facade / "domain.ttl", format="nt")
    require(
        set(graph.objects(M.package, O.originalFacade)) == {F.root},
        "wrong original root mapping",
    )
    require(
        (MEMBER, RDF.type, O.PinnedMembership) in graph
        and set(graph.objects(MEMBER, O.member)) == {M.package},
        "wrong pinned member",
    )
    require(
        set(graph.subjects(RDF.type, O.Module)) == set(MODULES.values()),
        "unexpected portable modules",
    )
    for rust_name, owner in MODULES.items():
        original_module = F[f"module:{rust_name}"]
        require(set(graph.objects(owner, O.owner)) == {M.package}, "wrong module owner")
        require(
            set(graph.objects(owner, O.originalFacade)) == {original_module}
            and (F.root, F["private-module"], original_module) in facade_graph,
            "wrong original module mapping",
        )

    evidence = compare()
    require(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n"
        == (LEXICAL / "comparison.json").read_text(),
        "native comparison changed",
    )
    rows = list(graph.query((HERE / "ownership.sparql.txt").read_text()))
    indexed = {str(row.spelling): row for row in rows}
    require(
        len(rows) == len(indexed) == len(SLUGS) and indexed.keys() == SLUGS.keys(),
        "expected one binding per portable data type",
    )
    require(
        set(graph.subjects(RDF.type, O.Type)) == {M[name] for name in SLUGS.values()}
        and set(graph.subjects(RDF.type, O.RustBinding))
        == {B[name] for name in SLUGS.values()},
        "unexpected portable types or target bindings",
    )
    for expected in evidence["publicDataRolePairs"]:
        name = expected["spelling"]
        row = indexed[name]
        component = expected["component"]
        native = component["module"]
        require(
            row.type == M[SLUGS[name]] and row.binding == B[SLUGS[name]],
            f"wrong portable or target identity for {name}",
        )
        require(row.owner == MODULES[native], f"wrong portable owner for {name}")
        require(
            row.facadeType == F[f"type:{native}:{name}"]
            and (F[f"module:{native}"], F["public-type"], row.facadeType)
            in facade_graph,
            f"wrong original facade identity for {name}",
        )
        for actual, known in (
            (row.kind, expected["normalizedKind"]),
            (row.role, expected["nativeRole"]),
            (row.original, "::".join(expected["originalDeclarationPath"])),
            (row.generated, "::".join(expected["generatedDeclarationPath"])),
            (row.componentKind, component["kind"]),
        ):
            require(
                actual == Literal(known),
                f"native projection evidence changed for {name}",
            )
        require(
            (row.component, RDF.type, O.Component) in graph
            and set(graph.objects(row.component, O.inMember)) == {MEMBER},
            f"wrong component membership for {name}",
        )
        for resource, digest, tag in (
            (row.graph, row.graphSha, "graph"),
            (row.overlay, row.overlaySha, "overlay"),
        ):
            expected_pin = component[tag]
            uri, raw = pin(BASE, package.parent / expected_pin["path"])
            require(
                resource == uri
                and str(digest)
                == hashlib.sha256(raw).hexdigest()
                == expected_pin["sha256"],
                f"wrong {tag} pin for {name}",
            )
    return rows


def main():
    graph = Graph().parse(
        ROOT / "oddlang/portable/core-structure-v1/ownership.ttl", format="turtle"
    )
    rows = verify(graph)
    for row in rows:
        print("\t".join(str(row[key]) for key in ("type", "owner", "spelling")))
    graph.set((M["retention-policy"], O.owner, M["probe-framing"]))
    try:
        verify(graph)
    except ValueError as failure:
        require(str(failure) == "wrong portable owner for Retention", str(failure))
        print(str(failure) + " (rejected)")
    else:
        raise ValueError("accepted a type moved to the wrong portable owner")


if __name__ == "__main__":
    main()
