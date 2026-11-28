# Read-only local file integrity

The [candidate manifest](../../../oddlang/portable/workspace-v7-local-file-integrity.json) adds an internal file policy to the selected canonical workspace. It uses the existing libc and SHA-256 providers without adding dependencies. The candidate checks read-only, close-on-exec descriptor access, ordinary file admission, maximum extent and a bounded read, exact verification extent, mode checks, and matching SHA-256 on stable inert files.

The independent probe runs on the pinned original and on a sealed generated native library. Ten guard observations distinguish I/O failures, descriptor rejection, size and mode failures, and a digest mismatch. A baseline file with two hard links is rejected. Compiling a mutation of the original link condition admits the second link and still rejects a third. Recovery carries that observed limit into separate generated graphs and compares both products with their original counterparts.

The projection uses Unix metadata. Its extra file-type preflight, nonblocking open and checked digest lookahead are safeguards beyond the stable-file parity claim. The bounded read rejects `u64::MAX` because a one-byte lookahead cannot be represented. The receipts cover effective ownership of fixture files and a failed mode check. Device handling, ancestor traversal and concurrent changes are outside the comparison. The policy is crate-private and is exercised by the fixture without adding a command, installation, packaging or network path.

Replay from clean checkouts. The compiler cache and the upstream test cache are writable, and provider dependencies must already be available offline.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin qualify-local-file-integrity -- \
  "$ODDLANG" "$COMPILER_CACHE" "$ORIGINAL" "$ORIGINAL_TARGET" /tmp/local-file-integrity.json \
  --framework-revision f658d2acff192008ad62c9079d8b14aadf81ccd3
```

The [macOS ARM64](aarch64-apple-darwin.json) and [Linux ARM64](aarch64-unknown-linux-gnu.json) receipts used the same baseline lock `24f1bda8604bfc74da3584f707538ff8b27997ccabc5f169139da5a4ed5c95b3`. Both matched the same original observations, mutation and native workspace pins. Three explicit source reads were denied before the generated builds.
