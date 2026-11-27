# Generated typed status client qualification

Build the graph-only typed status package with Oddlang at `ee4ba72af452aecb0d50c193c6b642552f8f0a79`.

```sh
oddlang build native-package rust oddlang/portable/native-status-typed-package.json /tmp/typed-status
cargo +1.97.1 build --manifest-path /tmp/typed-status/Cargo.toml --locked --offline --release --bins
```

The macOS ARM64 qualification isolates generation in the same Seatbelt allowlist used for the earlier client. The original and companion checkouts, original executables and network are outside the generation cell. The runner imports the original source and Cargo hashes for verification, stages only the pinned graph package closure, verifies the generated package lock and builds the two native binaries offline.

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
    --bin qualify-macos -- typed-status FRAMEWORK ORIGINAL ORIGINAL_BINS OUTPUT RECEIPT
```

The independent harness compares the generated client with the original native macOS ARM64 outputs. It covers the real activating reply with the original daemon, 17 framed socket controls, five ordered CLI guards, sixteen extended reply-shape cases and 33 additional nested variant cases. The latter cases exercise every top-level command and stream variant, including receiver claims and inspection records. All 49 reply-shape outputs agree in bytes and exit status. The Linux ARM64 release also matched the original across all 49 reply cases, 17 socket controls and a real original daemon. Linux compiled the sealed package originally emitted by `7fde562f4b4e027e6e35fbbf183750d04451e7d6`. The final reviewed compiler emits the same package lock and source bytes for these inputs. The original reply observations agree across both hosts.

The verified lane uses explicit state and socket arguments. Status discovery, peer checks with a different effective UID, production-duration timeout, installed-process checks, and daemon behavior remain separate platform or effect obligations. [macOS ARM64 receipt](generated-typed-macos-arm64.json) · [Linux ARM64 receipt](generated-typed-linux-arm64.json) · [Original macOS nested replies](typed-reply-variants-macos-arm64.json) · [Original Linux nested replies](typed-reply-variants-linux-arm64.json)
