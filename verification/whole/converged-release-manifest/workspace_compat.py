"""Expose the Rust parser witness to the later canonical workspace gate."""
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
SOURCE = "crates/marmish-native/src/native_manifest.rs"
VECTOR_SOURCE = "tools/release-version-vectors.json"
CONVERGENCE = runpy.run_path(str(HERE.parent / "converged-public-api/verify.py"))
ENGINE, LIMIT, SIX = (CONVERGENCE[key] for key in ("ENGINE", "LIMIT", "SIX"))


def manifest_oracle(args, host, scratch, product, build):
    artifacts = scratch / "parser-cargo-artifacts.jsonl"
    artifacts.write_bytes(build if isinstance(build, bytes) else build.encode())
    return SIX["product_witness"]("manifest-oracle", product, args.original, args.original_target,
                                  scratch, host, artifacts)
