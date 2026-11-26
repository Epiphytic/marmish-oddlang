"""Project a frozen release inventory into stable RDF resources and baseline obligations."""
import argparse
import json
from pathlib import Path
from urllib.parse import quote


def project(spec, inventory):
    rows = set()

    def node(*parts):
        return "<" + spec["namespace"] + ":".join(quote(part, safe="") for part in parts) + ">"

    def fact(subject, predicate, obj):
        rows.add(f"{subject} {predicate} {obj} .")

    def identity(resource, category, label):
        fact(resource, "a", "i:" + category)
        fact(resource, "rdfs:label", json.dumps(label, ensure_ascii=False))

    def obligation(subject, role, check):
        ident = subject[:-1] + ":obligation:" + quote(role, safe="") + ">"
        identity(ident, "Obligation", check)
        fact(ident, "i:appliesTo", subject)
        fact(ident, "i:role", json.dumps(role))

    release = node("release", inventory["commit"])
    identity(release, "Release", inventory["commit"])
    for workspace in inventory["workspaces"]:
        for package in workspace["packages"]:
            owner = node("package", package["manifest"])
            identity(owner, "Package", package["name"])
            fact(release, "i:contains", owner)
            for feature in sorted(package["features"]):
                feature_node = node("feature", package["manifest"], feature)
                identity(feature_node, "Feature", feature)
                fact(owner, "i:declares", feature_node)
            for target in package["targets"]:
                kind = "+".join(target["kind"])
                artifact = node("target", package["manifest"], kind, target["name"])
                identity(artifact, "CargoTarget", target["name"])
                fact(owner, "i:declares", artifact)
                fact(artifact, "i:kind", json.dumps(kind))
                for feature in target["required_features"]:
                    fact(artifact, "i:requires", node("feature", package["manifest"], feature))
                if kind == "lib":
                    obligation(artifact, "consumer", "Compile unchanged consumers and compare behavior")
                elif kind == "bin":
                    obligation(artifact, "process", "Compare process I/O, status and effects")
                else:
                    obligation(artifact, "build", "Build and exercise the selected target")
                if target["test"]:
                    obligation(artifact, "test", "Run original Cargo target test harness")
                if target["doctest"]:
                    obligation(artifact, "doctest", "Run unchanged documentation tests")
    for surface in spec["surfaces"]:
        family = node("surface", surface["id"])
        identity(family, "Surface", surface["contract"])
        fact(family, "i:owner", json.dumps(surface["owner"]))
        fact(release, "i:contains", family)
        for check in surface["checks"]:
            fact(family, "i:check", json.dumps(check))
        for path in inventory["file_groups"][surface["id"]]:
            artifact = node("file", path)
            identity(artifact, "Input", path)
            fact(family, "i:contains", artifact)
    for name in inventory["release_assets"]:
        artifact = node("asset", name)
        identity(artifact, "DistributionAsset", name)
        fact(release, "i:ships", artifact)
        obligation(artifact, "distribution", "Validate staged asset against upstream release requirements")
    for cell in inventory["baseline_cells"]:
        subject = node("cell", cell["target"], cell["profile"])
        identity(subject, "BaselineCell", cell["target"] + " " + cell["profile"])
        fact(release, "i:requires", subject)
        obligation(subject, "original", "Capture repeatable reference outcomes on a matching host")
    for program in spec["release"]["external_programs"]:
        subject = node("program", program["name"])
        identity(subject, "ExternalProgram", program["name"])
        fact(release, "i:references", subject)
        fact(subject, "i:scope", json.dumps(program["scope"]))
        fact(subject, "i:witness", json.dumps(program["witness"]))
        for field in ("source", "revision", "lock_sha256"):
            if field in program:
                fact(subject, "i:" + field, json.dumps(program[field]))
    prefixes = "@prefix i: <urn:oddlang:inventory:> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
    return prefixes + "\n" + "\n".join(sorted(rows)) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("spec", type=Path)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.write_text(project(json.loads(args.spec.read_bytes()), json.loads(args.inventory.read_bytes())))
