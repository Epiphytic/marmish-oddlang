# Checked Rust endpoint in the recovered workspace

The seven-member [workspace manifest](../../../oddlang/portable/workspace-v7-canonical-range-native-idioms.json) selects the exact Rust source-API binding used by the standalone endpoint package. The engine package keeps the recovered MissingReason and MissingRange graphs. Its range component now selects the binding that requires the public field `end: Option<u64>`. The other six member manifests, all Cargo dependency pins, and the previous sealed workspace and receipts keep their existing pins.

To reproduce, use the reviewed Oddlang runtime commit `b530e740a6eaef5e993192777463bf39e222c477`, its same-host compiled CLI, the retained source-recovered [predecessor](../canonical-recovered-range/README.md) product directory, and a fresh work path. The assembly uses a sealed stage and network-denied confinement.

```sh
python3 -B verification/whole/gap-range-role-workspace/compose.py --check
python3 -B verification/whole/gap-range-role-workspace/verify.py \
  FRAMEWORK CHECKED_COMPILER PREDECESSOR_PRODUCT NEW_WORK_DIRECTORY RECEIPT
```

The verifier replaces exactly three stage paths with the new workspace manifest, engine manifest and Rust binding. It requires byte equality for all generated Rust, Cargo manifests, Cargo.lock, and the six other member locks. The two adjusted locks record the new binding and manifest provenance. This equivalence ties the generated public endpoint to the existing native [endpoint caller and shared cases](../gap-range-endpoint/). The workspace build is an opt-in candidate.

Workspace lock SHA-256 `bb670bb4031c2c193d974f51a3343764caf46abc0097e57e85beb46db0217ae9`.

The [ARM64 macOS replay](aarch64-apple-darwin.json) compared 101 files and 45 directories. All 84 Rust files matched byte for byte. Exactly six provenance fields changed across the workspace and engine package locks. The corresponding sealed seven-member assembly on ARM64 Linux is pending guest recovery. The separate native endpoint corpus was already replayed against the byte-identical predecessor on both hosts.
