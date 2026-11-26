# Public engine fieldless choices

The replacement engine member adds the two crate-root data types
`Stream` and `SubmitDisposition` to the previously qualified engine
records and process-limit package. The same Cargo lock and previous
components supply the earlier interface.

Recovery sends exactly the two reviewed declaration spans from the pinned
engine model to the compiler. The generated package takes the resulting
choice graph and Rust naming overlay. The unchanged external caller
matches all variants, checks cloning and equality, and observes derived
Debug text and declaration-order ordinals. Separate compiler probes check
`Copy`, confirm the model module is private, and require E0277 for the
unsupported trait bounds, including Serde serialization and deserialization.

The mutation reorders both enums and adds Copy to SubmitDisposition in a
disposable full original source tree. The verifier compiles the original
and mutated engine libraries, then rebuilds both projected packages from
their graph inputs. Neither native caller enters an engine operation.

## Reproduce

Use the clean original, framework and companion revisions recorded in
the receipts. The verifier builds the compiler with the pinned native
Rust 1.97.1 toolchain. Keep the compiler and original Cargo targets
outside the checkouts.

```sh
lane=verification/whole/engine-fieldless-choices
python3 -B "$lane/verify.py" "$INPUT_REVISION" "$FRAMEWORK" "$ORIGINAL" \
  "$COMPILER_TARGET" "$ORIGINAL_TARGET" "$RECEIPT"
```

The generated package is
`oddlang/portable/engine-fieldless-choices-package.json`. The case
observations and graph and lock content are reproducible across builds.
Library artifact hashes identify the individual run.

Both native ARM64 runs matched the original for four value observations
and 17 trait and visibility probes in each of the baseline and mutation
cases. The graph, binding, and package lock digests matched across hosts.
The [Darwin receipt](aarch64-apple-darwin.json) and
[Linux receipt](aarch64-unknown-linux-gnu.json) identify the precise inputs.
