# Unified workspace with recovered release metadata and fixed vault bytes

The [workspace manifest](../../../oddlang/portable/workspace-v7-recovered-vault-engine.json) selects the five previously qualified members, the structurally recovered release metadata graph and the extended vault value package. Both native launchers retain the engine startup binding and native error API. The root Cargo lock adds the existing zeroize provider to the vault member's dependencies.

The macOS ARM64 verifier rebuilds the five members and both binaries from pinned inputs with network and source access denied. It reuses the earlier workspace observations and compiles the pinned original `strict_public_values` test against the generated vault member. A separate holdout calls `Secret::from_bytes` directly in both libraries, checks that the returned borrow is independent of the caller array, and verifies the value requires drop. The workspace verifier checks the release-report graph against the standalone source recovery receipt and compares both binaries' release-report output against the pinned original and the v7 cases.

The fixed vault test uses local byte literals. Credential storage, platform key access and vault helper behavior are separate obligations.

Use clean checkouts of the original at `af54339d250290490d62d4475a77539b7e739071` and Oddlang at `6e2d1ef00f286b6132b84e27a61805c659dfd437`.

```sh
python verification/whole/unified-recovered-vault/verify.py \
  /path/to/original /path/to/framework /path/to/compiler-target \
  /path/to/reference-target /tmp/unified-recovered-vault/receipt.json
```
