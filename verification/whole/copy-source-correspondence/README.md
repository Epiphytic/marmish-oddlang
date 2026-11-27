# Source-derived copy correspondence

The [portable recipe](../../../oddlang/portable/copy-source-correspondence-v1/template.json) leaves the comparison steps for `Launch::matches_manifest` vacant. The compiler receipt records ten typed comparisons recovered from the pinned upstream Rust method. The existing Rust overlay maps the observed field names to stable portable field identities. The resulting [package](../../../oddlang/portable/stream-copy-source-correspondence-package.json) uses the recovered graph and the other previously released copy relation components.

The verifier builds the CLI from an archive of the pinned framework revision, reruns observation and lowering, confirms that the recovered relation matches the released relation, and independently compiles upstream and generated libraries. It changes the upstream right-hand transfer operand to the manifest's server field in an archived checkout. A fresh compiler observation changes the corresponding graph edge and projected native method. An independent caller checks the original and mutated implementations against explicit expectations, including changed answers in both directions. Source-denied builds stage the generated contract, target hints and pinned dependencies.

Run from the repository root with the upstream revision `af54339d250290490d62d4475a77539b7e739071` and the [recorded framework revision](reference-aarch64-apple-darwin.json). The pinned Rust toolchain is 1.97.1 with the host's `rustc-dev` component installed.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/copy-manifest/Cargo.toml --bin copy-field-relations -- recover \
  /path/to/upstream /path/to/oddlang /tmp/copy-source-receipt.json
```

The current receipt qualifies the native macOS ARM64 profile. It covers the direct correspondence operation. Existing helper-context relations supply the package's ordered preflight checks.

The [Rust replay](rust-replay-macos-arm64.json) reproduced the complete archived receipt byte for byte. The pinned historical framework supplies its original capture driver. Rust performs graph recovery checks, source-denied projection, and the four native caller comparisons. Five of the 15 inputs changed results, covering both directions of disagreement.
