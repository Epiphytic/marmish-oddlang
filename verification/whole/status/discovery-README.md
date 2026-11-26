# Native status discovery qualification

Build with the reviewed Oddlang compiler at `a882f1cfe4a65880cb038f3ea32ccd7e7b271591`.

```sh
oddlang build native-package rust oddlang/portable/native-status-discovery-package.json /tmp/discovery-status
cargo +1.97.1 build --manifest-path /tmp/discovery-status/Cargo.toml --locked --offline --release --bins
```

The macOS ARM64 run built the pinned graph inputs with the existing Seatbelt allowlist. It denied reads of the original and companion trees and original executable directory, denied network and external writes, then built the generated binaries locked and offline. The runner compared the package lock and discovery obligation catalog before compiling.

```sh
verification/whole/status/macos-discovery-client.sh FRAMEWORK ORIGINAL ORIGINAL_BINS OUTPUT RECEIPT
```

The independent replay pins the native original binaries and uses fresh private home and temporary directories. It compares both client routes to a real original daemon, one using `--state-dir` without `--socket` and another using the environment default. The 43 private-fixture cases cover explicit bypass, profile and environment precedence, existing status guards, canonical paths, owner-mode or link violations in controlled same-UID fixtures, bounded documents, and named failures. It compares final paths, modes, link counts and file contents around each controlled client process. The existing typed response and same-UID exchange are retained.

Qualification covers client discovery, with representative filesystem effects and configured TCP loopback activation. Different-UID ownership and peer mismatches, installed startup, session and command effects, and full daemon generation remain separate. [macOS ARM64 receipt](discovery-macos-arm64.json)
