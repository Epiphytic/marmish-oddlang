# Source-derived copy frame guard order

The compiler observation derives nine encoder preconditions across seven variants. The recovered portable graph is byte-identical to the prior frame release. The [remaining obligations](../../../oddlang/portable/source-copy-frame-guards-v1/remaining-obligations.json) cover the Start and Manifest fragment behavior.

The verifier swaps the two preflight statements in the upstream Data encoder. Inputs that fail both rules change from `Limit` to `Offset`. Inputs with one failure retain its named error, valid encodings retain their bytes, and the decoder retains its original order. An independent byte and error oracle checks both upstream revisions and both source-free native packages. The verifier additionally covers the recovered offset and relative-field checks in the other six variants.

Run from this repository with upstream revision `af54339d250290490d62d4475a77539b7e739071` and the [recorded framework revision](reference-aarch64-apple-darwin.json). Rust 1.97.1 with the host's `rustc-dev` component is required.

```sh
python3 verification/whole/source-copy-frame-guards/verify.py \
  /path/to/upstream /path/to/oddlang /tmp/source-copy-frame-guards.json
```

The receipt covers macOS ARM64. Both product builds deny upstream sources and compiler observations.
