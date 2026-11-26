# Native status observations

Run from the companion repository with Python 3.11 or newer. Supply the directory containing the seven default release binaries built at the revision in the platform's bridge receipt. Preserve their native install names, executable permissions and private directory permissions. The runner checks all seven SHA-256 digests before starting the daemon.

```sh
python3 verification/whole/status/observe.py /path/to/macos/bin /tmp/reference.json
python3 verification/whole/status/observe.py --build-receipt verification/whole/linux/aarch64-unknown-linux-gnu/bridge.json /path/to/linux/bin /tmp/reference.json
```

Add `--candidate /path/to/generated/marmish` to exercise both clients against the same original daemon and all controlled cases. The mixed receipt records the candidate executable digest. Build provenance belongs with the package that produced the candidate.

The runner creates private state and a temporary Unix socket directory. A loopback TCP listener holds the relay connection before TLS negotiation so the original foreground daemon exposes its activation snapshot. The test compares status, output streams and process exit, then checks the daemon's shutdown report. A controlled Unix peer separately verifies the length-prefixed request and write EOF, supplies sixteen response variants, and checks the missing-socket diagnostic after closing the listener. These observations cover local status during activation. Ready-state authentication, command execution and bidirectional network transport require separate qualification.
