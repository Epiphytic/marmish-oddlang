# Native init with implicit state and saved config

The source-free package adds plain `marmish init` on an existing ready
file-backed profile with a grantless client configuration. The supported
configuration matches the profile emitted by upstream
`setup --role client --no-grants` with default settings and a direct WSS relay.
The package selects the Linux absolute XDG directory, Linux HOME fallback, or
macOS HOME path and verifies the private saved document before writing to the
profile. The CLI is also exercised with invalid, oversized and unsafe files.

Current format-7 journal reopening is qualified for recent application rows
that do not require native group recovery. The command reads actual SQLCipher
counts, advances both replay floors, reclassifies a prepared outbound row,
refreshes settings and binds the public command-engine references. The fixture
creates baseline profiles with the pinned upstream setup. It inserts control
rows through an independent SQLCipher caller and clears the binding first.
The same caller verifies the candidate before upstream revisits its state,
and then checks reuse with each binary running first.
The fixture inserts opaque message contents because init only observes their
presence and does not read or deliver them.

Generate with Oddlang `241d73ed6cd6d56f58ead84181fac7717467f822` using `oddlang build native-package rust
oddlang/portable/native-configured-init-package.json OUTPUT` and compare the
generated package lock with `oddlang/portable/native-configured-init-v1/package.lock`.
Use Rust 1.97.1 and locked offline release builds. The dependencies and public
Git coordinates are inherited from the prior encrypted init package. Build
`configured_store_probe.rs` as an independent caller using the pinned
SQLCipher rusqlite, serde_json and zeroize release dependencies. The replay
command is `python3 -B configured_init_probe.py ORIGINAL CANDIDATE INSPECTOR
RECEIPT --framework-revision REVISION`.

The scoped extension leaves privileged grants, nondefault command policy and
limits, native group recovery, stale rows, schema migrations, Tor and keychain
open. It refuses state that needs those recovery paths rather than reporting
success with incomplete effects.
