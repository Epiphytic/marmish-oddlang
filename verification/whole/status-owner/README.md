# Reverse native status qualification

The macOS runner starts from a clean pinned Oddlang checkout, the pinned original source and original native release binaries:

```sh
cargo +1.96.1 run --quiet --offline --locked --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin qualify-macos -- status-owner FRAMEWORK ORIGINAL ORIGINAL_BINS OUTPUT RECEIPT
```

It verifies the original source and dependency pins, compiles the exact framework, then generates the package and builds the native release executable from the graph and lock in an isolated staging cell. Original binaries, both source trees, Git objects, network and independent evidence are unavailable during generation. The runner verifies the Cargo and package locks before running the build.

The independent observer first runs the original native client against the original daemon, which serves the activation snapshot while a loopback relay deliberately stays before TLS. It then runs the same pinned client against the generated local listener. It compares stdout, stderr and exit, seven framed observations, and local cleanup after SIGTERM. The original provider shutdown report and generated local shutdown result are recorded separately. Candidate-only controls cover occupied paths, stale sockets, insecure parent directories and pathname length. The unrelated framework fixture checks admission and deadlines at smaller bounds. The native 64-client saturation and an externally observable stopping snapshot remain unqualified. Candidate binary hashes identify the tested build instances, while graph and lock digests and process results can be compared across runs.

This is a read-only activation lane. Active/degraded provider snapshots, profile leases and automatic endpoint discovery, installed daemon behavior, other IPC operations and full provider shutdown are open.

The [macOS ARM64 receipt](generated-macos-arm64.json) records the clean graph build, exact original-client exchange, seven frames and both shutdown observations.

The [Linux ARM64 receipt](generated-linux-arm64.json) rebuilds the same isolated generated package locally with `cargo +1.97.1 build --release --locked --offline`. On a native Linux host, run `qualify.py ORIGINAL_BINS GENERATED/target/release/marmish-status-owner RECEIPT --framework-revision PIN --build-receipt ORIGINAL_LINUX_RECEIPT` from the clean companion checkout. Both releases match on the reverse interaction and candidate admission controls.
