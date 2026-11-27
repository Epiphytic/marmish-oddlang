# Generated public diagnostics

The [portable graph](../../../oddlang/portable/native-diagnostics-v1/graph.json) defines a Boolean state, three public operations, ordered updates to that state and a caller-owned value, and conditional or unconditional structured events. Event fields refer to arguments, literals and host/package metadata. [Source origin](../../../oddlang/portable/native-diagnostics-v1/origin.json) · [Rust binding](../../../oddlang/portable/native-diagnostics-v1/rust.json)

The [seven-member workspace](../../../oddlang/portable/workspace-v7-offline-cli-diagnostics.json) adds that module to the qualified offline CLI package. It retains the same Cargo dependency lock and the other six package locks. In the native crate, the root module, its generated module index, and the new diagnostic implementation are the only generated source files that change. The old native package lock is verified against the parent workspace before comparing these files.

With Oddlang built from `cff611af71aa9ceaf649d6e3d8b97d181eaf351b` and provider dependencies cached, run from the repository root:

```sh
out=$(mktemp -d)
oddlang build native-workspace rust oddlang/portable/workspace-v7-offline-cli-diagnostics.json "$out/workspace"
cmp "$out/workspace/workspace.lock" oddlang/portable/workspace-v7-offline-cli-diagnostics/workspace.lock
cargo +1.97.1 build --release --locked --offline --workspace --manifest-path "$out/workspace/Cargo.toml"
```

The qualifier stages only the graph inputs and dependency providers, denies access to the original diagnostic module and downstream probe during generation and compilation, then recovers the graph from the original and checks every byte. It compiles the [same outside caller](../public-diagnostics/probe.rs) against the generated library. The four [original scenarios](../public-diagnostics/README.md) check exit status, final stage and stderr, including a closed pipe. The Rust library artifact hashes identify each tested build. Agreement is assessed on the observed public results.

The [Darwin ARM64 receipt](aarch64-apple-darwin.json) and [Linux ARM64 receipt](aarch64-unknown-linux-gnu.json) cover the same workspace lock, compiler and graph inputs. Both generated libraries reproduced all four original scenarios on their own host. Each sealed build denied all twelve source and caller access probes.

Run the full source-isolated qualification from a clean companion checkout with the pinned upstream binaries and an external target directory for each build.

```sh
cargo +1.96.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin qualify-six-member -- "$FRAMEWORK" "$ORIGINAL" "$RECEIPT" --diagnostics \
  --framework-revision cff611af71aa9ceaf649d6e3d8b97d181eaf351b \
  --compiler-target "$COMPILER_TARGET" --original-target "$ORIGINAL_TARGET" --original-bin-dir "$ORIGINAL_BIN_DIR"
```
