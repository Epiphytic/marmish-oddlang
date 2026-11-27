# Original stream and bridge on Linux ARM64

The ARM64 receipts came from a native CPU Linux VM on Apple Silicon with Rosetta disabled and no host mounts. The Ubuntu 24.04.5 image SHA-256 was `50e24e1fb79e89116e939d90aa7738f7af48c442fa1bc2510131e0b39c50348e`. Rust 1.97.1 compiled both the upstream package and the independent caller inside the guest. All 403 public stream cases, 31 wire replays and three mutation controls matched in dev and release. Rust and Python exchanged compressed data in both directions. All 29 unchanged upstream stream unit tests passed in release.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin public-stream-replay -- run /path/to/pinned-oddlang /path/to/original /tmp/stream-receipts
cargo +1.97.1 test --manifest-path /path/to/original/Cargo.toml --release --locked --offline -p marmish-stream
```

The Rust runner builds the oracle from Git revision `08373b871aeac0f76cb1675748fd604cea2a79a2`. The baseline Oddlang checkout must contain that commit in its Git object database. The host receipt records the Rust oracle revision and executable digest. To recheck saved dev and release receipts, use `public-stream-replay verify FRAMEWORK TARGET RECEIPT_DIRECTORY`.

The stream runner checks the framework revision from [the reviewed baseline pin](../baseline-framework.json) using the shared Rust checker and the source revision and lock of the original. Use the updated driver required by `verification/whole/bridge/baseline.py` for the separate bridge run. Supply a writable target directory under a trusted parent. The VM's default 0002 build umask produced a group-writable worker and the original rejected Init with `invalid_policy`. The builder sets 0077 for new artifacts.

```sh
python3 -B verification/whole/bridge/baseline.py --upstream /path/to/original \
  --oddlang /path/to/oddlang-with-linux-lock-probe --target-dir /path/to/release-build \
  --receipt /tmp/original-bridge.json
python3 -B verification/whole/linux/effects.py \
  /path/to/oddlang-with-linux-lock-probe \
  /path/to/release-build/aarch64-unknown-linux-gnu/release /tmp/effects.json
```

The bridge receipt covers 11 executable builds and two runs of five original CLI cases. Init succeeds at both 256 and 1,048,576 descriptors on this guest. The traced Init uses an existing empty state directory and a read-only root. The tracer requires sudo, strace and bubblewrap inside the disposable guest. It verifies that the bridge runs as the original user with zero effective capabilities, checks the live descriptor limits via procfs, and observes exact Init reply, held lock, durable files and EOF. An outside create fails with EROFS. The independent POSIX byte-range probe conflicts with the original Linux OFD lock. A BSD flock probe succeeds on the same file while the bridge holds that lock. The corrected verifier also passed both repeats on macOS, where the lock uses flock. The public corpus runs inside the guest. The generated Copy Data peer results appear below.

The Linux effects runner delegates procfs and strace observations to the Rust `bridge-observations` verifier. It follows the selected bridge PID from its successful exec through exit, verifies an acquired OFD lock, and checks effective credentials, descriptor limits and open state files. Run its fixture tests on either host with `cargo +1.97.1 test --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin bridge-observations`.


## Sealed original stream runs

The macOS qualification host compared all 128 sealed observations for each separately compiled Linux ARM64 dev and release caller. The executable reports its build binding, and the judge verifies its digest before and after replay. The judge probes isolation in both directions. A sandboxed host process receives EPERM when it tries to read the capsule, and the Linux guest has no shared mounts and cannot see a randomized host sentinel. SSH targets the local guest on loopback with forwarding disabled. Only input case names, operations and arguments are delivered to the guest.

Build once in the Linux guest for each profile. Run the judge on macOS with the private capsule already held there. The published receipts include the commitment and pass count.

```sh
python3 -B verification/whole/linux/sealed.py build "$FRAMEWORK" "$ORIGINAL" dev "$GUEST_BUILD"
python3 -B verification/whole/linux/sealed.py judge "$FRAMEWORK" "$SSH_CONFIG" "$GUEST_ALIAS" \
  "$GUEST_BUILD/build.json" "$QUALIFICATION_CAPSULE" /tmp/arm64-dev.json
```


## Native Linux AMD64

The archived dev and release stream receipts matched the frozen public observation digest on a native hosted x86-64 runner. A second run built the seven-package workspace binaries, the MDK probe, the fault frontend, two installed-check targets, and the separate installed fixture. Both release executables returned the expected version and help. The unchanged process-limits test passed. The bridge passed five cases twice with both mutation controls rejected. The directory contains separate receipts for each run. [Stream CI](https://github.com/Epiphytic/oddlang/actions/runs/45772674699) · [Release and bridge CI](https://github.com/Epiphytic/oddlang/actions/runs/45773302127)

An independent AMD64 run rebuilt only the original bridge and worker from the same source pin. Version 1 Init returned the same exact success reply at inherited soft and hard descriptor limits of 256 and 1,048,576. The unprivileged bridge held its Linux OFD lock while its SQLite, WAL, shared-memory, lock and jobs descriptors were open. The tracer saw executable entry, a successful OFD lock syscall and successful exit. Closing stdin released the lock. The witness covers bridge Init through EOF. [Effects receipt](x86_64-unknown-linux-gnu/effects.json) · [Build provenance](x86_64-unknown-linux-gnu/effects-build.txt) · [Hosted run](https://github.com/Epiphytic/oddlang/actions/runs/45775359148)

## Generated Copy Data on ARM64

The Linux guest generated Rust and TypeScript from the pinned Data graph. Each role matched 253 public cases (628 observations) and 112 sealed cases (288 observations). All six peer directions exchanged 36 encoded records. Changing the graph tag changed the generated behavior. Withholding the library hints preserved the generated source hashes. Other Copy variants and the complete stream, client and server contracts remain open.

In the guest, provision Rust 1.97.1, the pinned original and its offline Cargo dependencies, Node 22 with type stripping, Clang and bubblewrap. Use the generator at the original qualification framework pin. Use the Linux-capable qualifier whose source hashes appear in the public receipt. This command runs public cases. On the recorded VM Ubuntu required privileged mount setup. The driver dropped to the guest user with zero effective capabilities before exercising the target.

```sh
python3 -B "$QUALIFIER/qualify_tagged.py" --linux-privileged-mounts \
  --oddlang "$GENERATOR" --upstream "$ORIGINAL" --graph "$DATA_GRAPH" --hints "$HINTS" \
  --original-profile release --original-target aarch64-unknown-linux-gnu \
  --original-revision af54339d250290490d62d4475a77539b7e739071 --receipt /tmp/data-public.json
```

For the sealed comparison, use the same guest command from the macOS judge, supplying absolute guest paths as arguments. The judge appends the receipt path and blind input mode. Use a new guest receipt path per run. The guest executes the public cases and the blind inputs supplied by the host. It reports raw adapter results to the judge over SSH. The judge compares expected outcomes on the host and publishes only counts and commitments. Copy the guest receipt alongside the judge receipt and compare its SHA-256 against `guestReceiptSha256`.

```sh
python3 -B verification/whole/linux/sealed.py judge-data "$FROZEN_FRAMEWORK" "$SSH_CONFIG" \
  "$GUEST_ALIAS" "$QUALIFICATION_CAPSULE" "$NEW_GUEST_RECEIPT" /tmp/sealed-data.json \
  env "PATH=$GUEST_NODE_BIN:$GUEST_CARGO_BIN:/usr/bin:/bin" python3 -B "$GUEST_QUALIFIER/qualify_tagged.py" \
  --linux-privileged-mounts --oddlang "$GUEST_GENERATOR" --upstream "$GUEST_ORIGINAL" \
  --graph "$GUEST_DATA_GRAPH" --hints "$GUEST_HINTS" --original-profile release \
  --original-target aarch64-unknown-linux-gnu --original-revision af54339d250290490d62d4475a77539b7e739071
```

The default-feature original workspace release suite recorded 886 passed, one failed, and five ignored on this guest. The failing engine test returned `UnsafePath` when opening its policy. Staging both the byte-identical executable and working directory under private paths made the isolated test pass. Changing only one path still failed. The per-target counts, reproduction command and four isolated outcomes are in [the suite receipt](../suite/cargo-release-aarch64-unknown-linux-gnu.json). A separate entrypoint process-limits probe passed for both original release binaries and rejected each mutation. [Process-limits receipt](../suite/process-limits-aarch64-unknown-linux-gnu.json)
