# Recover the engine output-gap record

This lane recovers `MissingRange` from the full original engine
`model.rs`, the sibling crate root, and native Rustdoc JSON. The selectors
pin source and evidence bytes and assign stable identities to the record,
its fields and the linked `MissingReason` choice. The record graph and
Rust overlay are compiler outputs. The linked choice's graph and overlay
come from the separately qualified [source recovery](../gap-reason-recovery/).

The original commit is `af54339d250290490d62d4475a77539b7e739071`. The
compiled Rustdoc recipe observes the original and a disposable copy that
adds only `#[serde(deny_unknown_fields)]` to `MissingRange`. Independent
recovery changes the unknown-field policy from ignore to reject. The
baseline graph and binding are compared as JSON values against the
[authored contract](../../../oddlang/portable/engine-gap-range-v1/). The
verifier checks the baseline native source digests against that package
and requires the mutation's other graph facts to agree. The existing
authored manifest and receipts retain their original pins.

The [earlier standalone qualification](../gap-range-record/) compared
the source-mutated original against the authored package with an
independent native caller. The earlier gate archived the generated
mutation rows only after they matched the compiled original mutation.
This lane reuses those rows and the separate original baseline receipt. It compiles
the same caller and trait checks against both newly recovered outputs.
The source-denied native build receives graph, overlay and Cargo inputs.
The probe source is staged only after generation. The raw model and
Rustdoc files remain outside the generated package and publication.

## Reproduce

Use the pinned upstream checkout, the framework runtime recorded in the
host receipt, the native Rust 1.97.1 toolchain, and cached Cargo inputs.
Use external scratch directories for the build targets and outputs.

```sh
lane=verification/whole/gap-range-recovery
python3 -B "$lane/rustdoc.py" "$ORIGINAL" "$RUSTDOC_TARGET" "$RUSTDOC_EVIDENCE"
python3 -B "$lane/verify.py" --framework-revision "$RUNTIME_PIN" \
  "$FRAMEWORK" "$COMPILER_TARGET" "$ORIGINAL" "$RUSTDOC_EVIDENCE" "$OUTPUT"
```

Rustdoc JSON identifies a native compiler run, so its raw digest can
differ between hosts and tool invocations. The verifier checks the
host-specific runner-attested evidence and selectors, rechecks the JSON
profile, and compares the portable and
native observations. It also rejects a baseline selector paired with
the mutant source and Rustdoc input before any output appears.
