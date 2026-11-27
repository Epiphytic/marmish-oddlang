"""Stage and count the two original release manifest unit tests."""
from pathlib import Path
import runpy


HARNESS = "marmish-native/lib/marmish_native"
NAMES = ["native_manifest::tests::shared_release_version_vectors", "native_manifest::tests::strict_vectors"]
SOURCES = {"manifestTests": "6012150c30678a0ccaddd4475249132528d174ab0bf62750dd9c9e5073dcfae8",
           "releaseVersionVectors": "b07696eda226714f8bd5f6a725be662d1a7df4ec1693c2bfd30a78df0dc3b6c1"}


def stage(original, product):
    whole = Path(__file__).resolve().parents[1]
    tool = runpy.run_path(str(whole / "unified-mdk/workspace_compat.py"))["product_witness"]
    selected = tool("suite-parser", original, product)
    return selected["leaf"], selected["prefix"], selected["vector"]


def witness(inside, product, prefix, passing_names, check_execution):
    command = ("cargo", "test", "--locked", "--offline", "--release", "-p", "marmish-native", "--lib", "--")
    results = {}
    for name in NAMES:
        registered = prefix + "::" + name.split("::", 1)[1]
        raw = inside(*command, "--exact", registered, cwd=product).decode()
        check_execution(registered, 1, raw, passing_names(raw))
        results[name] = 1
    return results
