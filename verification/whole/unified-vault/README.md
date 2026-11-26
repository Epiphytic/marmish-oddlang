# Vault values and engine limits in the v7 workspace

`oddlang/portable/workspace-v7-vault-engine.json` builds five sibling crates and both native binaries. It retains the v7 release report and the public native error API. The vault crate adds the checked namespace, purpose, key reference, label and metadata values. The engine crate adds process limit initialization. Both binaries invoke that initialization before argument parsing.

On ARM64 macOS, stage the manifest's pinned JSON, Turtle and Cargo locks, then generate and compile all workspace members under a sandbox that denies network access and reads of the original, companion and framework trees, the reference build and temporary checkouts under `/private/tmp`. The verifier checks those read denials before generation. After the build, it runs the 11 unchanged upstream copy protocol tests and nine exact frame cases, checks the vault value caller and private constructor diagnostics, replays the public native error caller and both sets of release-report cases, and compares engine library and startup observations against the pinned native original. The startup fault probe forces the first file-descriptor limit read to fail.

The check covers the compiled local APIs and process startup observations listed in the receipt. Vault credential storage, platform key access, the full engine API and the full seven-member workspace remain open.

Use a clean checkout of the framework at `7ed1c29acde2cc4cfdc58c2489935e703470ae70` and the original at `af54339d250290490d62d4475a77539b7e739071`. Supply Cargo target directories for the compiler and the reference build. The verifier builds the original release binaries and both external callers with Cargo using the supplied reference target directory.

```sh
python verification/whole/unified-vault/verify.py \
  oddlang/portable/workspace-v7-vault-engine.json \
  /path/to/original /path/to/framework /path/to/compiler-target \
  /path/to/reference-target /tmp/unified-vault/receipt.json
```
