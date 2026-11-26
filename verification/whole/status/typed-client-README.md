# Generated typed status client qualification

Build the graph-only typed status package with Oddlang at `7fde562f4b4e027e6e35fbbf183750d04451e7d6`.

```sh
oddlang build native-package rust oddlang/portable/native-status-typed-package.json /tmp/typed-status
cargo +1.97.1 build --manifest-path /tmp/typed-status/Cargo.toml --locked --offline --release --bins
```

The macOS ARM64 qualification isolates generation in the same Seatbelt allowlist used for the earlier client. The original and companion checkouts, original executables and network are outside the generation cell. The runner imports the original source and Cargo hashes for verification, stages only the pinned graph package closure, verifies the generated package lock and builds the two native binaries offline.

```sh
verification/whole/status/macos-typed-client.sh FRAMEWORK ORIGINAL ORIGINAL_BINS OUTPUT RECEIPT
```

The independent harness compares the generated client with the original native macOS ARM64 outputs. It covers the real activating reply with the original daemon, 17 framed socket controls, five ordered CLI guards, sixteen extended reply-shape cases and 33 additional nested variant cases. The latter cases exercise every top-level command and stream variant, including receiver claims and inspection records. All 49 reply-shape outputs agree in bytes and exit status. The sixteen extended original observations also agree across native macOS ARM64 and Linux ARM64.

The verified lane uses explicit state and socket arguments. Status discovery, peer checks with a different effective UID, production-duration timeout, installed-process checks, and daemon behavior remain separate platform or effect obligations. [macOS ARM64 receipt](generated-typed-macos-arm64.json) · [Original nested observations](typed-reply-variants-macos-arm64.json)
