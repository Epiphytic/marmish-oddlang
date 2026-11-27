# Authorized owner local submit

The portable workspace binds the private async owner listener, existing canonical request encoder, and scoped local queue. The foreground owner checks the current client role and trusted remote, exact retained room and route, and native membership and Welcome evidence. It derives the local account from the active native identity. The local reply follows the queue commit on the already keyed owner connection.

The request and invocation IDs contain a six-byte unsigned big-endian Unix seconds prefix. The original client's fresh ID generator uses Unix seconds and its command wire reader decodes those six bytes. The application graph binds that clock and its allowed future offset. The handle carries the SHA-256 of the encoded request frame.

Run the local witness with a compiler that supports the workspace.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin authorized-submit-offline -- \
  /path/to/oddlang /path/to/fixture-target
```

The witness supplies a synthetic retained-room row and disposable SQLCipher database. It exercises the generated local selection, role and peer checks, canonical encoding, first commit and identical receipt after reopening the connection. It runs with network access denied. It covers the retained-room lookup and local receipt boundary. Native membership and Welcome checks remain a separate runtime qualification.

The pinned workspace passed the native offline witness on macOS ARM64 with Rust 1.97.1. The result used compiler SHA-256 `46147d9ba643769e125e3102a3677980925931e86fa29b6e6e243d92adfcfbf8`. Its graph selects the observed reply flags and receipt version 1. The compiler also exercised a separate gauge projection with receipt version 7 and different field names. [Framework PR](https://github.com/Epiphytic/oddlang/pull/151)
