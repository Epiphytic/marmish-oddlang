# Native status observations

Run from the companion repository with Python 3.11 or newer. Supply the directory containing the seven default release binaries built at the revision in the platform's bridge receipt. Preserve their native install names, executable permissions and private directory permissions. The runner checks all seven SHA-256 digests before starting the daemon.

```sh
python3 verification/whole/status/observe.py /path/to/macos/bin /tmp/reference.json
python3 verification/whole/status/observe.py --build-receipt verification/whole/linux/aarch64-unknown-linux-gnu/bridge.json /path/to/linux/bin /tmp/reference.json
```

Add `--candidate /path/to/generated/marmish` to exercise both clients against the same original daemon and all controlled cases. The mixed receipt records the candidate executable digest. Build provenance belongs with the package that produced the candidate.

The runner creates private state and a temporary Unix socket directory. A loopback TCP listener holds the relay connection before TLS negotiation so the original foreground daemon exposes its activation snapshot. The test compares status, output streams and process exit, then checks the daemon's shutdown report. A controlled Unix peer separately verifies the length-prefixed request and write EOF, supplies sixteen response variants, and checks the missing-socket diagnostic after closing the listener. These observations cover local status during activation. Ready-state authentication, command execution and bidirectional network transport require separate qualification.

## Extended reply shapes

The independent extended corpus exercises sixteen representations, including tuple lengths, unit and structured variants, nested unknown fields, and both typed extensions in one response. The pinned macOS and Linux ARM64 clients produced identical output and exit status on every case. Each original receipt binds the executable and corpus digests. Compare a generated client with the archived original receipt as follows.

```sh
cd verification/whole/status
cargo +1.96.1 run --quiet --locked --offline --manifest-path ../canonical-public-data/Cargo.toml \
  --bin status-replies -- extended /path/to/original/marmish /tmp/original-shapes.json --reference-status macos-arm64.json
cargo +1.96.1 run --quiet --locked --offline --manifest-path ../canonical-public-data/Cargo.toml \
  --bin status-replies -- extended /path/to/generated/marmish /tmp/candidate-shapes.json --compare reply-shapes-macos-arm64.json
```

Comparison writes the actual output and mismatching case names before returning failure. The initial generated status client differed on nine cases, including the three-field tuple, nested typed results and their combination. This corpus supplements the real activating-daemon and framing observations. It samples a subset of the nested command and stream variants.
