# Local relay fixture

The pinned upstream workflow uses Deaddrop commit `b38b2841b081952dc7728da391fe0e26fc7c6181`. For deterministic transport checks we use its explicit loopback debug mode. Its three reference tests cover an authenticated publish that finishes after sender disconnect, challenge ordering in a pipelined exchange, and process-level startup, logging and shutdown. In these tests ephemeral ports, challenge values and wall-clock timestamps may vary. The required response ordering, acceptance and redaction checks stay fixed. These checks qualify the local relay fixture. Actual original/generated Marmish peers still require separate runs.

From a clean checkout of that Deaddrop commit, populate Cargo's pinned dependency cache with `cargo +1.97.1 fetch --locked`. Then run the Rust fixture verifier.

```sh
cargo +1.96.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin relay-fixture -- /path/to/deaddrop /tmp/deaddrop-build
```

## Foreground owner and direct relay admission

The native macOS and Linux ARM64 owners reject a direct plaintext relay. With an explicit loopback WSS endpoint, the listener observes a TLS handshake record, and the original client discovers the activating owner using its state directory. The TLS fixture presents a fresh certificate for the loopback IP. OpenSSL verifies that certificate against the explicit test trust anchor. The original owner rejects it. After the fixture closes the listener, the owner exits with the relay connection error, prints its shutdown report and removes the Unix socket. These receipts establish the failed-activation path. A ready owner and authenticated exchange require separate receipts.

Run the observer with all seven default binaries from the platform’s pinned bridge build, Python and OpenSSL. Temporary private keys stay in the per-run directory and are removed on exit.

```sh
python3 -B verification/whole/relay/owner_tls.py /path/to/original/bin verification/whole/bridge/aarch64-apple-darwin.json /tmp/owner-tls.json
```

For Linux ARM64 use `verification/whole/linux/aarch64-unknown-linux-gnu/bridge.json`. [macOS receipt](owner-tls-macos-arm64.json) · [Linux receipt](owner-tls-linux-arm64.json)
