"""Stage and count the two original release manifest unit tests."""
from pathlib import Path
import hashlib
import runpy


HARNESS = "marmish-native/lib/marmish_native"
NAMES = ["native_manifest::tests::shared_release_version_vectors", "native_manifest::tests::strict_vectors"]
SOURCES = {"manifestTests": "6012150c30678a0ccaddd4475249132528d174ab0bf62750dd9c9e5073dcfae8",
           "releaseVersionVectors": "b07696eda226714f8bd5f6a725be662d1a7df4ec1693c2bfd30a78df0dc3b6c1"}


def stage(original, product):
    whole = Path(__file__).resolve().parents[1]
    tool = runpy.run_path(str(whole / "unified-mdk/workspace_compat.py"))["product_witness"]
    selected = tool("manifest-selection", original, product / "marmish-native")
    tests = selected["tests"].encode()
    vector_path = Path(selected["vector"])
    vectors = (original / vector_path).read_bytes()
    actual = {"manifestTests": hashlib.sha256(tests).hexdigest(),
              "releaseVersionVectors": hashlib.sha256(vectors).hexdigest()}
    if actual != SOURCES:
        raise ValueError("original parser test bytes or included vectors changed")
    leaf, prefix = Path(selected["leaf"]), selected["prefix"]
    target = product / vector_path
    target.parent.mkdir(exist_ok=True)
    with target.open("xb") as stream:
        stream.write(vectors)
    with leaf.open("ab") as stream:
        stream.write(b"\n" + tests)
    return leaf.relative_to(product).as_posix(), prefix, vector_path.as_posix()


def witness(inside, product, prefix, passing_names, check_execution):
    command = ("cargo", "test", "--locked", "--offline", "--release", "-p", "marmish-native", "--lib", "--")
    results = {}
    for name in NAMES:
        registered = prefix + "::" + name.split("::", 1)[1]
        raw = inside(*command, "--exact", registered, cwd=product).decode()
        check_execution(registered, 1, raw, passing_names(raw))
        results[name] = 1
    return results
