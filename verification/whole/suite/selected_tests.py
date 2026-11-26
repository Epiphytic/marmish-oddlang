"""Stage pinned upstream library tests as consumers of generated crates."""
import hashlib
import subprocess

from inventory import PIN

VAULT = "marmish-vault/lib/marmish_vault"
ENGINE = "marmish-engine/lib/marmish_engine"
VAULT_CASE = "tests::strict_public_values"
LOW = "process_limits_tests::lower_soft_hard_and_descriptor_flags_are_unchanged"
LINUX = "process_limits_tests::linux_process_limits_are_unchanged"
VAULT_SOURCE = "crates/marmish-vault/src/lib.rs"
ENGINE_SOURCE = "crates/marmish-engine/src/process_limits_tests.rs"


def cases(host):
    return {VAULT: [VAULT_CASE], ENGINE: sorted([LOW, LINUX] if host.endswith("linux-gnu") else [LOW])}


def upstream(root, path):
    return subprocess.check_output(["git", "-C", root, "show", f"{PIN}:{path}"])


def unique_offset(data, marker):
    if data.count(marker) != 1:
        raise ValueError(f"expected one upstream section starting with {marker!r}")
    return data.index(marker)


def stage(original, product):
    vault = upstream(original, VAULT_SOURCE)
    module = vault[unique_offset(vault, b"#[cfg(test)]\nmod tests {"):]
    engine = upstream(original, ENGINE_SOURCE)
    start = unique_offset(engine, b"const CASE:")
    mac = unique_offset(engine, b'#[cfg(target_os = "macos")]\nfn high_soft()')
    low = unique_offset(engine, b"#[test]\nfn lower_soft_hard_and_descriptor_flags_are_unchanged()")
    stop = unique_offset(engine, b"#[test]\nfn normalized_startup_executes_offline_exact_bytes_and_status()")
    if not start < mac < low < stop:
        raise ValueError("unexpected upstream process limit module order")
    # Keep upstream test functions and their private helpers byte-for-byte.
    # Only the imports and outer module are supplied by the integration harness.
    parts = {"vaultValueModule": module, "engineLimitHelpers": engine[start:mac],
             "engineLimitCases": engine[low:stop]}
    digests = {k: hashlib.sha256(v).hexdigest() for k,v in parts.items()}
    if digests != EXPECTED:
        raise ValueError(f"upstream partial harness bytes changed: {digests}")
    files = {"marmish-vault": b"pub use marmish_vault::*;\n" + module,
             "marmish-engine": (b"mod process_limits_tests {\nuse marmish_engine::initialize_process_limits;\n"
                               b"use std::{os::fd::AsRawFd, process::Command};\n"
                               + parts["engineLimitHelpers"] + parts["engineLimitCases"] + b"}\n")}
    added = set()
    for name, body in files.items():
        path = product / name / "tests" / "selected_upstream.rs"
        path.parent.mkdir()
        path.write_bytes(body)
        added.add(path.relative_to(product).as_posix())
    return added, digests


EXPECTED = {
    'vaultValueModule': '5389218be38e60288371fe63a79ae71d2e9dbb8408a34bc53e8010f03117359c',
    'engineLimitHelpers': 'f27e24a823249163fa4e7d99634429edf50488b8706094bcbfe19bdbb599b11f',
    'engineLimitCases': 'c8344cac4a33a60ec110a385c4a61087ccfee2f8a52e41395cab4513c852d570',
}
