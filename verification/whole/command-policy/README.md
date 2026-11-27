# Command restriction conformance

Pass the pinned upstream repository, a Cargo target cache and the generated workspace to the replay.

```sh
cargo run --locked --offline --manifest-path verification/whole/command-policy/Cargo.toml -- \
  qualify /path/to/marmish /path/to/cargo-target /path/to/generated-workspace \
  /tmp/policy-verification.json
```

The build uses an archive of upstream commit `af54339d250290490d62d4475a77539b7e739071`, its pinned Rust toolchain and its locked offline Cargo dependencies. The first observer feeds each raw policy through the native configuration loader, policy normalizer, engine policy validator and pure executable decision method. The second observer compiles the complete generated workspace with a thin observer appended to the foreground module and invokes its parser, principal validation and executable decision method. Both observers use the same two admitted synthetic principals. The replay checks every result against the native observation and a separately specified expected outcome.

Cases cover named paths, empty and omitted lists, deny precedence, multiple entries for one principal, principal isolation, keys encoded as hex and npub, canonical and invoked names, non-UTF-8 path bytes, single and recursive wildcards, escapes, named and positional JSON records, version types, schema errors, path syntax, size bounds and match budget exhaustion including alias bytes. A separate file test calls the generated foreground room_config loader on disposable 0600 config files, including omitted, null, malformed, empty and named policies. Runtime transport, executable launch and stream fidelity require separate evidence.

On macOS ARM64, independent source recovery and two workspace renders produced identical schema bytes and workspace trees. The final workspace lock is `2efe4cea5e65e5918024f7f79465c92297e47dc019a66399b4e5894eb6eb172c`. A locked offline Rust 1.97.1 release build of all workspace binaries succeeded. Both generated executables printed `marmish-native 0.3.1` with `--version`. The build used a warm dependency cache and rebuilt the native crate. [Policy comparison receipt](aarch64-apple-darwin.json)

The Rust validator requires Cargo to report a new compilation of the observed native library and its observer executables in each disposable source tree. The receipt identifies the exact observer bytes. The portable policy inputs, including raw JSON literals, and independent expected decisions are frozen from the original qualification suite at companion commit `51064fa`. Each row in [policy-cases.json](policy-cases.json) is one tab-delimited observer input, and [room-cases.json](room-cases.json) holds the 11 file cases.
