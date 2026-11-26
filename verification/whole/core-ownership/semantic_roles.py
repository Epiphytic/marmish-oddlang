"""Resolve the six portable core types using only the ownership and domain graphs."""

import hashlib
import json
from pathlib import Path

from rdflib import Graph, Literal, Namespace, RDF, URIRef, XSD

HERE = Path(__file__).resolve().parent
PORTABLE = HERE.parents[2] / "oddlang/portable"
O = Namespace("urn:oddlang:structural-ownership:")
M = Namespace("urn:marmish:structure:core:")
C = Namespace("urn:oddlang:core:")
PIN = "https://github.com/Epiphytic/marmish-oddlang/blob/036349fc69ab5b850df2006c16ec2518be95a254/oddlang/portable/"
MANIFEST_SHA256 = "89292d721acd668379be55e762b963bfac16f0c57fad916c450ca14f0658e1d4"

# Independent expected correspondence for each public type, including aggregates.
TYPES = {
    "retention-policy": ("value-domain", O.domain),
    "relay-set": ("endpoint-set", O.domain),
    "probe-envelope": ("json-envelope", O.domain),
    "configuration-failure": ("value-domain", O.errorCase),
    "framing-failure": ("json-envelope", O.errorCase),
    "frame-limits": ("json-envelope", O.parameterPredicate),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def expectations(kind, raw):
    if kind == "endpoint-set":
        document = json.loads(raw)
        return {O.domain: {(URIRef(document["id"]), None)}}
    graph = Graph().parse(data=raw, format="turtle")
    cls = {"value-domain": C.OptionalUnsignedRange, "json-envelope": C.JsonEnvelope}[kind]
    root, = graph.subjects(RDF.type, cls)
    result = {
        O.domain: {(root, None)},
        O.errorCase: {(case, None) for case in graph.subjects(RDF.type, C.NamedError)},
    }
    if kind == "json-envelope":
        limits = {C.rawBytes, C.wireBytes, C.recordLineBytes}
        for prop in limits:
            value, = graph.objects(root, prop)
            require(isinstance(value, Literal) and value.datatype == XSD.integer, "invalid envelope limit")
        result[O.parameterPredicate] = {(p, root) for p in limits}
    return result


def resolve(graph):
    raw_manifest = (PORTABLE / "core-package.json").read_bytes()
    require(hashlib.sha256(raw_manifest).hexdigest() == MANIFEST_SHA256, "unexpected core manifest")
    manifest = json.loads(raw_manifest)
    inputs = {c["kind"]: c["graph"] for c in manifest["components"]}
    query = (HERE / "semantic-roles.sparql.txt").read_text()
    rows = list(graph.query(query))
    require(set(graph.subjects(RDF.type, O.Type)) == {M[k] for k in TYPES}, "unexpected type inventory")
    require({row.type for row in rows} == {M[k] for k in TYPES}, "missing semantic roles")
    for slug, (kind, role) in TYPES.items():
        selected = [row for row in rows if row.type == M[slug]]
        component, = graph.objects(M[slug], O.semanticComponent)
        spec = inputs[kind]
        raw = (PORTABLE / spec["path"]).read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        require(digest == spec["sha256"], "component graph digest mismatch")
        component_kind, = graph.objects(component, O.componentKind)
        member, = graph.objects(component, O.inMember)
        require(component_kind == Literal(kind) and member == M["canonical-member"], "wrong semantic component")
        for predicate in (O.domain, O.errorCase, O.parameterSubject, O.parameterPredicate):
            allowed = {O.parameterSubject, O.parameterPredicate} if role == O.parameterPredicate else {role}
            require(predicate in allowed or not list(graph.objects(M[slug], predicate)), "unexpected semantic role")
        require(all(row.role == role and str(row.digest) == digest and
                    row.source == URIRef(PIN + spec["path"]) for row in selected),
                "wrong component pin or semantic role")
        expected = expectations(kind, raw)[role]
        require(len(selected) == len(expected) and {(r.term, r.subject) for r in selected} == expected,
                f"wrong semantic selections for {slug}")
    return rows


def remove_native_bindings(graph):
    for binding in list(graph.subjects(RDF.type, O.RustBinding)):
        graph.remove((binding, None, None))
    for hint in (O.originalFacade, O.rustOverlay, O.rustOverlaySha256):
        graph.remove((None, hint, None))


def main():
    graph = Graph().parse(PORTABLE / "core-structure-v1/ownership.ttl")
    complete = resolve(graph)
    remove_native_bindings(graph)
    require(resolve(graph) == complete, "semantic resolution depends on native bindings")
    print(f"{len(TYPES)} types resolve to {len(complete)} domain, case and parameter selections without native bindings")
    wrong = M["configuration-failure"]
    graph.set((wrong, O.semanticComponent, M["probe-envelope-domain"]))
    try:
        resolve(graph)
    except ValueError as failure:
        require(str(failure) == "wrong semantic component", str(failure))
    else:
        raise ValueError("accepted a different aggregate error component")
    print("rejected substituted aggregate error component")


if __name__ == "__main__":
    main()
