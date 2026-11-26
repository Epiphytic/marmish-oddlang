# Native engine process-limit slice

The scoped engine library exports `marmish_engine::initialize_process_limits() -> marmish_engine::Result<()>` and its reachable error `EngineError::WorkerUnavailable`. The [portable policy](../../../oddlang/portable/workspace-engine-limits/graph.json) declares the macOS soft descriptor ceiling of 65,536 and the Linux no-op. The [workspace manifest](../../../oddlang/portable/workspace-engine-limits.json) brings that library into the v7 source-free workspace. Both normal executables run the library operation before parsing arguments. The existing workspace releases and their locks retain their pins.

To generate the standalone library, run the matching Oddlang compiler:

```sh
oddlang build native-package rust oddlang/portable/engine-process-limits-package.json /tmp/native-engine-limits
cargo +1.97.1 build --manifest-path /tmp/native-engine-limits/Cargo.toml --locked --offline --release
```

To qualify the combined artifact, use a clean Oddlang checkout at
`aaf3f497d7b0fdcc543aa51dbdf19fbf517d261b`, the pinned original checkout,
and the selected original native release binaries.

```sh
python3 -B verification/whole/workspace/verify.py \
  PATH_TO_ODDLANG PATH_TO_ORIGINAL /tmp/engine-limits-receipt.json \
  --release engine-limits --original-bin-dir PATH_TO_ORIGINAL_RELEASE_BINS
```

The verifier builds and locks all four members with the upstream checkout,
oracles, and network inaccessible. See [the workspace provider-cache prerequisites](../workspace/README.md).
After the release builds, an independent Rust caller checks the original and
generated libraries with soft limits 1024, 65,536, and 65,537 in separate
children. It verifies the resulting limits, existing descriptors with and
without close-on-exec, and repeated calls. The ordinary CLI binaries run the
same help, version, error, and release-info cases with a high inherited limit.
On macOS an own-process libc fault forces the real entrypoint to return its
startup error ahead of an invalid argument. The regular v7 replay runs against
the same generated executables.

Scope ends at process limit initialization and its reachable error. The
remainder of `EngineError`, engine bootstrap, descriptor enumeration,
spawn gates, worker executables, and the separate native error-family
composition require their own qualification.

[macOS ARM64 receipt](aarch64-apple-darwin.json) · [Linux ARM64 receipt](aarch64-unknown-linux-gnu.json)

Both receipts pin the exact companion inputs and framework build and record the
same 45 retained workspace tests, all 29 original stream tests, and the native
engine observers. The highest inherited soft limit becomes 65,536 on macOS
and stays at 65,537 on Linux. Each host records its own hard limit and verifies
that it is preserved. The same generated client and daemon appear throughout
each host's receipt.
