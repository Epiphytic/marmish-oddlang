# Native stream wire operation replay

Run from a clean original Marmish checkout at `af54339d250290490d62d4475a77539b7e739071` with Rust 1.97.1 and its locked Cargo registry cache. Use the reviewed Oddlang native package compiler at `25ea63ce1aea475d7a3a3fa050a4042c46345409`. The package manifest pins the structural graph, operation graph, both native bindings, shared stream declarations, integrity block, Copy Launch, public case enums and Cargo dependencies.

Build the standalone library from this checkout.

```sh
/path/to/oddlang build native-package rust oddlang/portable/stream-wire-package.json /tmp/stream-wire-package
cargo +1.97.1 build --manifest-path /tmp/stream-wire-package/Cargo.toml --locked --offline --release
```

Reproduce the independent native comparison.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/stream-wire/Cargo.toml --bin replay-wire-operations -- \
  --oddlang /path/to/oddlang \
  --framework-revision 25ea63ce1aea475d7a3a3fa050a4042c46345409 \
  --original /path/to/original \
  --receipt /tmp/stream-wire-operations.json
```

The runner builds the fully generated package and the original in separate locked offline release builds. It compiles the same native caller against each library. The caller compares exact canonical bytes, named errors and messages, request and response acceptance, SHA-256 results, clock boundary values, and the Launch and integrity block dependencies. The receipt binds the generator executable used, input and output hashes and original provenance. It shortens long successful frames only after comparing the complete native output bytes. The archived macOS ARM64 run matched all 81 case records. [Native receipt](reference.json)
