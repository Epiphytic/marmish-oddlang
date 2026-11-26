# Original stream and bridge on Linux ARM64

The ARM64 receipts came from a native CPU Linux VM on Apple Silicon with Rosetta disabled and no host mounts. The Ubuntu 24.04.5 image SHA-256 was `50e24e1fb79e89116e939d90aa7738f7af48c442fa1bc2510131e0b39c50348e`. Rust 1.97.1 compiled both the upstream package and the independent caller inside the guest. All 403 public stream cases, 31 wire replays and three mutation controls matched in dev and release. Rust and Python exchanged compressed data in both directions. All 29 unchanged upstream stream unit tests passed in release.

```sh
python3 -B verification/whole/linux/replay.py /path/to/pinned-oddlang /path/to/original /tmp/stream-receipts
cargo +1.97.1 test --manifest-path /path/to/original/Cargo.toml --release --locked --offline -p marmish-stream
```

The stream runner checks the framework revision from `verification/whole/framework_pin.py` and the source revision and lock of the original. Use the updated driver required by `verification/whole/bridge/baseline.py` for the separate bridge run. Supply a writable target directory under a trusted parent. The VM's default 0002 build umask produced a group-writable worker and the original rejected Init with `invalid_policy`. The builder sets 0077 for new artifacts.

```sh
python3 -B verification/whole/bridge/baseline.py --upstream /path/to/original \
  --oddlang /path/to/oddlang-with-linux-lock-probe --target-dir /path/to/release-build \
  --receipt /tmp/original-bridge.json
python3 -B verification/whole/linux/effects.py \
  /path/to/oddlang-with-linux-lock-probe/examples/marmish/portable-bridge/verify.py \
  /path/to/release-build/aarch64-unknown-linux-gnu/release /tmp/effects.json
```

The bridge receipt covers 11 executable builds and two runs of five original CLI cases. Init succeeds at both 256 and 1,048,576 descriptors on this guest. The traced Init uses an existing empty state directory and a read-only root. The tracer requires sudo, strace and bubblewrap inside the disposable guest. It verifies that the bridge runs as the original user with zero effective capabilities, checks the live descriptor limits via procfs, and observes exact Init reply, held lock, durable files and EOF. An outside create fails with EROFS. The independent POSIX byte-range probe conflicts with the original Linux OFD lock. A BSD flock probe succeeds on the same file while the bridge holds that lock. The corrected verifier also passed both repeats on macOS, where the lock uses flock. The Linux stream run uses the public corpus. Sealed Linux stream cases and generated Linux peer qualification are separate obligations.
