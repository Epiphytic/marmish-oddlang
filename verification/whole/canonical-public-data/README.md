# Canonical offline public data candidate

This separate seven-member workspace adds public `marmish_engine::Stream`, `marmish_engine::SubmitDisposition`, `marmish_engine::MissingReason` and `marmish_native::mdk_adapter::RuntimeStatus` to the canonical candidate. It composes already qualified portable inputs. The MissingReason component pins the recovered graph and Rust overlay. The native member uses the exact RuntimeStatus package.

The engine adds a direct registry dependency on Serde with the derive feature. The workspace already pins this registry closure, so the root Cargo lock changes only at the engine's dependency list. The standalone engine lock follows the canonical registry resolution. No workspace sibling dependency changes. The five unrelated workspace member locks remain byte-identical.

The verifier regenerates the prior workspace with the same compiler and checks its canonical lock. It builds one sealed candidate, audits all 140 frozen import spellings, and runs all four added data types through external callers of the single build. RuntimeStatus serialization and trait denials, fieldless enum cases and traits, and all 17 MissingReason wire outcomes and its Copy denial are compared with independently produced native receipts. The callers do not invoke services.

Use clean original, framework, and companion checkouts, Rust 1.97.1, and the locked offline dependencies.

```sh
python3 -B verification/whole/canonical-public-data/verify.py INPUT_REVISION \
  FRAMEWORK ORIGINAL COMPILER_TARGET NEW_WORK_DIRECTORY RECEIPT
```
