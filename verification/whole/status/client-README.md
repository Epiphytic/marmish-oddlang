# Generated status client qualification

The macOS ARM64 runner requires a clean pinned Oddlang framework checkout, the pinned original checkout, the original release binary directory, an empty output location, and a fresh receipt path.

```sh
verification/whole/status/macos-client.sh FRAMEWORK ORIGINAL ORIGINAL_BINS OUTPUT RECEIPT
```

The runner validates the source and Cargo pins, builds the exact framework compiler with Rust 1.97.1, then isolates graph-only workspace generation and locked offline binary builds behind a Seatbelt data allowlist. The original and companion trees, original binaries, Git databases, oracle and network are outside the generation cell. Writes are confined to that cell. The runner compares the Cargo, package and workspace locks before building. A separate process after build uses the pinned original daemon and original client as a differential oracle, exercises the frame controls and tests explicit CLI argument guard precedence. Three known incompatible reply shapes are checked and recorded as open. The existing parser transcript is replayed separately as a regression check.

Status reports the activation snapshot from a real original daemon. The independent harness holds the loopback relay connection before TLS negotiation. The generated client talks only to the daemon's private socket. The receipt covers this mixed direction on macOS ARM64 and [the limited client lane](../../../oddlang/portable/native-status-v1/). The receipt identifies the actual tested binary by its per-run hash. Rust and Clap embed staging paths, so repeats compare observations and graph and lock bytes while the executable hash can vary.

The [Linux ARM64 capture](generated-linux-arm64.json) independently rebuilt the same generated workspace offline and exercised the real activation snapshot and all 17 controlled peer cases. Its recorded Oddlang revision predates the final link validation change. The status workspace, package locks and generated source bytes agree across revisions.
