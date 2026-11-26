# Build the stream block codec

From the companion root, with Oddlang framework revision `274d8e2b43d4ef97e58eac0cdd7cd39ea8274cbe`:

```sh
oddlang build native-package rust oddlang/portable/stream-block-package.json /tmp/stream-codec
cmp oddlang/portable/stream-block-package-v1/package.lock /tmp/stream-codec/package.lock
cargo +1.97.1 build --manifest-path /tmp/stream-codec/Cargo.toml --release --offline --locked
```

The emitted library exports six public numeric constants, the stream error and result types, and the codec module. The source-free package represents the block-codec part of marmish-stream. The window and wire modules and file-copy protocol have separate requirements.

The shared declarations and their Rust bindings come from extraction at a pinned source revision. The selector supplies identity and native names. Multiplication, the reference between two limits, error display templates, payload types, result target and variant order are recovered from the Rust declarations. Recover those inputs with framework revision `7abab129d8c2d2d95324387bf0f9b84309b8f166`:

```sh
oddlang recover shared-declarations /path/to/original oddlang/portable/stream-shared-v1/selection.json /tmp/stream-values
```

The separately authored integrity-block graph defines the accepted domains, checksum, representations, validation order and decoding constraints. The Rust overlay selects native type names and error constructors. Its conformance tests compare native behavior and allow equivalent compressed bytes when the wire contract admits them.
