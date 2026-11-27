# Source-derived Launch record layout

The importer expands the pinned library with Rust 1.97.1 during recovery and observes eleven field writes in the private encoder that supplies the Launch digest. The graph records their order, portable encodings and type evidence. The final codec argument, SHA-256 domain, validation order and namespace operation come from the separately qualified Launch contract.

The observed low-32-bit length cast is compatible with the checked length prefix on the validated Launch domain. Its two operand lengths are bounded at 4,096 before hashing. The importer also recovers both byte tag tables from the expanded enums and checks them against the release declarations. Only the stable graph and its digest-bound witness are archived. Compiler expansion is transient. The verification uses Rust 1.97.1 with RUSTC_BOOTSTRAP=1 and -Zunpretty=expanded only during recovery. The published expansion pin and sandbox receipt were qualified on macOS and Linux ARM64. Both produced the same compiler expansion digest and the same 201 changed outcomes after the source mutation. Linux generation and compilation run inside the [private build namespace](../../../verification/whole/copy-launch/README.md).

The verifier recovers the layout from the pinned source, replaces every record slot in the digest graph, checks the resulting release pin, and builds with the original and companion sources inaccessible. It then runs a native caller against both releases and an independent byte oracle. A second run swaps two writes in a disposable source clone, independently builds the modified library, and compares it against the recovered and recompiled graph.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/copy-manifest/Cargo.toml --bin check-launch-layout -- /path/to/original /path/to/pinned/oddlang /tmp/record-layout.json
```
