# Generated candidate build

The baseline compiler is Oddlang revision `8a20ea283d0d43da42f054a7c7774eb9e6a20da7`. The portable input tree comes from companion revision `53ece04bad4d12f0a6846ecb652493be7bf92a55`. The companion uses the native Rust 1.97.1 toolchain on macOS ARM64.

Run the builder from the companion checkout. Supply the pinned Oddlang checkout and a new private export location. Give the denial probes real existing files from the original checkout, reference artifacts, and oracle checkout.

```sh
python3 -B verification/whole/basic-command-interop/build.py \
  "$ODDLANG_CHECKOUT" "$EXPORT" \
  --deny originalSource "$MARMISH_CHECKOUT/crates/marmish-native/src/lib.rs" \
  --deny originalBinary "$MARMISH_BIN_DIR/marmish" \
  --deny oracleFixture "$ORACLE_FIXTURE"
```

The driver builds Oddlang separately. It stages the manifest closure, registry archives verified against Cargo.lock and shallow Git repositories pinned to the locked third-party commits. It uses a new workspace and Cargo home. Seatbelt restricts filesystem reads to the build workspace, toolchain and system paths and denies networking. The driver tests read denials before generation. The receipt documents the allowed inputs and denial probes. The driver does not collect a syscall trace.

Cargo builds all seven generated members from an empty target directory. The driver checks the workspace and member manifests, lock files and emitted sources before and after compilation. The export contains both executable artifacts, the generated workspace manifest and locks, and a SHA-256-bound receipt. Keep the generated and original artifacts in separate directories when selecting participants for the command harness.

The initial candidate has these role boundaries.

| Role | Candidate coverage |
| --- | --- |
| Client `marmish` | Binds `status`, `init` and `release-info`. Remote invocation (foreground and detached submission) and `wait` are unbound. |
| Daemon `marmishd` | Binds `daemon` and `release-info`. The foreground owner requires an explicit prepared file profile. Its lifecycle specifies the engine as absent. |
| Provisioning | Generated `init` handles a native file profile. `setup` and `keychain-authorize` are unbound. |
| Worker and credential helper | The package generates no worker or keychain helper executable. |

These boundaries derive from the [native package manifest](../../../oddlang/portable/marmish-native-canonical-offline-package.json), [client binding](../../../oddlang/portable/native-cli-terminal-v1/marmish-cli.json), [daemon binding](../../../oddlang/portable/native-cli-terminal-v1/marmishd-cli.json) and [foreground owner graph](../../../oddlang/portable/workspace-v6/foreground-graph.json). The inventory identifies prerequisites for both mixed directions. The interoperability gate requires independently observed command completion, byte streams and exit status.
