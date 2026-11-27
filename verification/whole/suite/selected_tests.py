"""Stage pinned upstream library tests as consumers of generated crates."""
from pathlib import Path
import runpy


VAULT = "marmish-vault/lib/marmish_vault"
ENGINE = "marmish-engine/lib/marmish_engine"
VAULT_CASE = "tests::strict_public_values"
LOW = "process_limits_tests::lower_soft_hard_and_descriptor_flags_are_unchanged"
LINUX = "process_limits_tests::linux_process_limits_are_unchanged"

PROTOTYPE = "marmish-prototype/lib/marmish_prototype"
READER = "secret_request_tests::private_reader_wipes_consumed_spans"
READER_PREFIX = "buffer::__projection_0::"
READER_LEAF = "marmish-prototype/src/group_0/__projection_0.rs"
PRODUCT_WITNESS = runpy.run_path(str(Path(__file__).resolve().parents[1] / "unified-mdk/workspace_compat.py"))["product_witness"]


MDK = "marmish-mdk/lib/marmish_mdk"
MDK_CASE = "tests::synthetic_json_and_ndjson_are_bounded_and_structural_only"
MDK_PREFIX = "metadata::__projection_2::"
MDK_DENIED_SOURCES = ("lib.rs", "tests.rs")
MDK_SOURCE_SHA = "04916e33e73f6e8856688823f8af5af965677a7403e195477de9f4a19d1a6ad7"
MDK_CONTROL = {
    "selector": MDK_PREFIX + MDK_CASE + "_absent",
    "cargoExit": 0, "executed": 0, "coverageRejected": True,
}


def stage_mdk(original, product):
    helper = runpy.run_path(str(Path(__file__).resolve().parents[1] / "mdk-json-input/verify.py"))
    digest = helper["stage_case"](product / "marmish-mdk", original)
    if digest != MDK_SOURCE_SHA:
        raise ValueError("the named upstream MDK case changed")
    return "marmish-mdk/" + helper["LEAF"].as_posix(), digest


def zero_mdk_control(inside, product, command, passing_tests):
    selector = MDK_CONTROL["selector"]
    raw = inside(*command, "--exact", selector, "--format", "pretty", cwd=product).decode()
    names = passing_tests(raw)
    if names or raw.count("test result: ok. 0 passed; 0 failed; 0 ignored;") != 1:
        raise ValueError(f"absent MDK filter did not exit with zero tests: {raw}")
    try:
        check_execution(selector, 1, raw, names)
    except ValueError:
        return dict(MDK_CONTROL)
    raise ValueError("the coverage gate accepted a successful zero-test invocation")


def cases(host, include_reader=False, include_mdk=False):
    partial = {VAULT: [VAULT_CASE], ENGINE: sorted([LOW, LINUX] if host.endswith("linux-gnu") else [LOW])}
    if include_reader:
        partial[PROTOTYPE] = [READER]
    if include_mdk:
        partial[MDK] = [MDK_CASE]
    return partial


def stage_reader(original, product):
    selected = PRODUCT_WITNESS("reader-test", original)
    with (product / READER_LEAF).open("ab") as leaf:
        leaf.write(selected["module"].encode())
    return selected["bodySha256"]



def check_execution(name, runs, raw, passing_names):
    if passing_names != [name] * runs or raw.count("test result: ok. 1 passed; 0 failed; 0 ignored;") != runs:
        raise ValueError(f"expected {runs} single-case results for {name}: {raw}")


def stage(original, product):
    report = PRODUCT_WITNESS("suite-vault-engine", original, product)
    return set(report["paths"]), report["digests"]
