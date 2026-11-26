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

## macOS ARM64 evidence

The [build receipt](generated-build-aarch64-apple-darwin.json) records the seven compiled members, 362 verified registry archives, four pinned Git providers, and eight read-denial probes. The [candidate preflight](generated-preflight-aarch64-apple-darwin.json) records exact status and streams for six network-denied invocations. Foreground submission, detached submission, `wait`, `setup`, and `keychain-authorize` exit 125 with empty stdout and `command operation unavailable` on stderr. The daemon reports a required prepared profile. Its test state and all five client directories stayed empty and were removed. Replacing either executable or the build receipt was rejected before the preflight produced an observation.

Use the expected receipt digest from the original build handoff to replay the preflight.

```sh
python3 -B verification/whole/basic-command-interop/candidate_preflight.py \
  "$EXPORT" "$PREFLIGHT_RECEIPT" --build-receipt-sha256 "$EXPECTED_BUILD_RECEIPT_SHA256"
```

See the [Claude review](../../../docs/reviews/basic-command-generated-build.md) for the provenance checks and remaining scope.

## Profile setup candidate

Select `workspace-profile-setup.json` using `--workspace`, with the full Git IDs of the compiler and companion inputs supplied through `--framework-pin` and `--companion-pin`. The driver inventories CLI roles from the selected manifest closure. This workspace binds setup in addition to initialization, status and release reporting.

The local profile comparison invokes both builds with fresh file credentials and network access denied. It compares exact bytes after replacing only the freshly generated public identities, checks configuration permissions, and verifies workspace removal. Each relay spelling has twelve cases, including explicit grant, trust, rejection, revocation and reopening the granted server with init. The onion URL in this comparison is an offline fixture. [Local comparison receipt](local-profile-setup-aarch64-apple-darwin.json)
