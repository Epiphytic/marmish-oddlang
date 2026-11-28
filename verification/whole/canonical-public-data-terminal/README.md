# Combined public data terminal regression

This gate exercises 36 immediate help and version exits for each of `marmish` and `marmishd`. It uses a fixed allowlist from the archived terminal corpus: five exact root forms and 31 `help COMMAND` descriptions. A command in a `help COMMAND` row is an argument to the help parser.

For each role, 46 other archived argv rows are excluded. These include parser-rejection cases, direct command forms, and auxiliary help or version forms outside the allowlist. The separately archived post-parse control is also excluded. The receipts identify each excluded vector. The run invokes no service or remote operation.

The verifier links the release executables from the retained sealed candidate, checks that its workspace, data libraries, and full recorded Cargo library closure survive the link, and verifies both original binary pins. Both native binaries run with network access denied and an independently checked empty home. The verifier compares exact stdout, stderr, and exit status with the archived result before recording output digests.

From a clean checkout, use the retained build directory produced by the [combined data gate](../canonical-public-data/README.md) and the original release executables pinned by the [terminal receipt](../terminal-prefix/README.md).

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin terminal-case-inventory -- qualify \
  SEALED_BUILD ORIGINAL_BIN_DIRECTORY RECEIPT
```

Both ARM64 hosts matched all 72 observations against the pinned original for workspace lock `b50bd2a6896f7f253b4ed6005844f7cc48704dc3663568815df811e60b68f3ab`. [ARM64 macOS](aarch64-apple-darwin.json) · [ARM64 Linux](aarch64-unknown-linux-gnu.json)
