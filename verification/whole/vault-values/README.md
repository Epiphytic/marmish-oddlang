# Standalone vault value library

The [portable package](../../../oddlang/portable/vault-value-domain-package.json) builds a dependency-free `marmish-vault` 0.1.0 library in Rust 2021. It includes the public `Namespace`, `Purpose`, `KeyRef`, `Label`, `Metadata` and `Error` value API. The compiler records the six declaration shapes. Reviewed graph declarations specify validation and exact errors.

An unchanged native caller exercises all byte-sized version arguments for `KeyRef`, the complete ASCII identifier boundary, text spelling, UTF-8 byte limits, Cc controls and the explicit bidi and separator ranges. It checks public metadata, borrowed getters, all error categories and Display values. Separate native compiler probes reject attempts to construct checked private fields directly.

The verifier tightens the original and portable label byte limit from 128 to 127 in independent temporary copies. It checks the original source change against the old expectation, then compares both revised native libraries with an independent oracle. Exact Unicode byte-length boundary rows distinguish the two releases. Both generated packages deny upstream and framework source and compiler observations during build.

Run from this repository against upstream revision `af54339d250290490d62d4475a77539b7e739071`, the [recorded framework revision](reference-aarch64-apple-darwin.json) and Rust 1.97.1 with its host `rustc-dev` component.

```sh
python3 verification/whole/vault-values/verify.py \
  /path/to/upstream /path/to/oddlang /tmp/vault-values.json
```

The receipt qualifies macOS ARM64. Secret ownership, platform providers, helper commands, and unified workspace integration remain open. The verifier uses synthetic labels and identifiers from its own offline corpus and performs no platform credential operations.
