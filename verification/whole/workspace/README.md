# Two-member native workspace qualification

The verifier requires clean checkouts of both Git revisions in the receipt. It builds the pinned framework, checks every manifest and component input, then assembles and compiles the native workspace with upstream files and network access unavailable. After that clean release build succeeds, it injects the original core integration test file and the byte-exact upstream codec and window test modules at native privacy scope. The receipt records all seventeen test names and the test hashes.

```sh
python3 -B verification/whole/workspace/verify.py \
  /path/to/pinned/oddlang /path/to/pinned/marmish /tmp/native-workspace-receipt.json
cmp /tmp/native-workspace-receipt.json verification/whole/workspace/reference-aarch64-apple-darwin.json
```

The native macOS ARM64 and Linux ARM64 receipts have identical workspace, test module and test-name observations. Linux uses a separate network namespace, an allowlist of files exposed with bubblewrap, and a process with zero effective capabilities. Its replay command uses the same inputs as the macOS run. Compare its output with `reference-aarch64-unknown-linux-gnu.json`. Individual member releases also carry independent public caller and mutation evidence.

## Three-member workspace

`oddlang/portable/workspace-v2.json` pins the native core, stream and command-line packages. The stream library exposes the codec, windows, copy Launch and canonical wire interfaces together. The command-line package provides the two CLI parsers and the explicit-socket status client.

```sh
oddlang build native-workspace rust oddlang/portable/workspace-v2.json /tmp/oddlang-marmish-build
cargo +1.97.1 build --workspace --locked --release --offline --manifest-path /tmp/oddlang-marmish-build/Cargo.toml
python3 -B verification/whole/workspace/verify.py /path/to/pinned/oddlang /path/to/original /tmp/v2.json --release v2
```

Use a fresh destination. Provision the pinned Rust toolchain and prepopulate the registry cache for every dependency in the lock, including target-specific dependencies used by Cargo metadata. The verifier compares staged inputs, generated source inventories and locks before a release build with the original sources and network inaccessible. Then it injects the unchanged core, codec, windows and wire suites and checks all 25 results. The wire test caller supplies four private bounds referenced by upstream tests. They are test inputs. The release build has already finished before the verifier adds them.

The native crate currently has CLI and local status projections. The generated daemon parser, command processing, relay and installed runtime have separate obligations. The core and stream libraries in this workspace can also be consumed independently.
