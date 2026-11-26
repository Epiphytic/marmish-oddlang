# Foreground local provider lifetime

This native package adds a scoped provider lifetime to the v5 foreground-status contract. It opens and retains the encrypted journal, SQLCipher account storage, the account-device runtime, and the Nostr SDK context while the daemon serves local status. The SDK owns the real loopback TLS attempt and its notification task. The output report observes actual provider deactivation, client and notification shutdown, journal close, and runtime cleanup. The explicit no-engine role reports the engine stop as complete and quiescence as false.

The target is a private, current-format file profile prepared by either initializer, with no stored command configuration and one explicit direct loopback WSS relay. It covers held TLS activation, its signal and certificate-failure shutdowns, and selected invalid-store starts. The transport stops at TLS. WebSocket messaging, provider readiness, nonempty or historical stores, replay-floor advancement, configured command execution, and installed service behavior remain open.

Use Rust 1.97.1 and an offline Cargo cache populated with the lock's packages and four Git provider commits. Follow the [v5 cache instructions](../workspace/README.md#workspace-v5) with this native package instead of a Cargo workspace.

```sh
oddlang build native-package rust oddlang/portable/native-foreground-lifecycle-package.json /tmp/marmish-lifecycle
cargo +1.97.1 build --manifest-path /tmp/marmish-lifecycle/Cargo.toml --locked --offline --release
python3 -B verification/whole/foreground-lifecycle/verify.py FRAMEWORK ORIGINAL ORIGINAL_BINS RECEIPT
```

The verifier builds with upstream source, original binaries, the oracle, and network access excluded. Afterward the original and generated daemons each hold real encrypted database file descriptors during a live TLS ClientHello. Both real clients use automatic discovery. The contender is refused while the owner holds its lease. The observer checks TLS EOF, exact signal and trust-failure reports, lease release, and independent encrypted readback and credential preservation. Corrupt and unsafe stores fail before listening and preserve their originally bound runtime for retry.
