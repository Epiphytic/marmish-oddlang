# Canonical offline public data candidate

This separate seven-member workspace adds public `marmish_engine::Stream`, `marmish_engine::SubmitDisposition`, `marmish_engine::MissingReason` and `marmish_native::mdk_adapter::RuntimeStatus` to the canonical candidate. It composes already qualified portable inputs. The MissingReason component pins the recovered graph and Rust overlay. The native member uses the exact RuntimeStatus package.

The engine adds a direct registry dependency on Serde with the derive feature. The workspace already pins this registry closure, so the root Cargo lock changes only at the engine's dependency list. The standalone engine lock follows the canonical registry resolution. No workspace sibling dependency changes. The five unrelated workspace member locks remain byte-identical.

The verifier regenerates the prior workspace with the same compiler and checks its canonical lock. On each host it builds one sealed candidate, audits all 140 frozen import spellings, and runs all four added data types through external callers of the single build. Public imports rise from 77 to 81 on both hosts. RuntimeStatus serialization and trait denials, fieldless enum cases and traits, and all 17 MissingReason wire outcomes and its Copy denial are compared with independently produced native receipts. The callers do not invoke services.

Use clean original, framework, and companion checkouts, Rust 1.97.1 for the native witnesses, and the locked offline dependencies. The original revision is `af54339d250290490d62d4475a77539b7e739071`. The [combined Oddlang source](https://github.com/Epiphytic/oddlang/tree/900cfa517d0136fa80c2b5e92ccf9b1a3b2a71ab) is fixed at `900cfa5`.

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin qualify-canonical-public-data -- INPUT_REVISION \
  FRAMEWORK ORIGINAL COMPILER_TARGET NEW_WORK_DIRECTORY RECEIPT
```

The Darwin gate builds the compiler from its pinned source checkout and records the resulting artifact separately from the source revision. The Linux gate uses `--prebuilt PATH --prebuilt-sha256 SHA256` with the independently verified compiler from the [Linux framework receipt](https://github.com/Epiphytic/oddlang/blob/51ffc9ca4a1b35aaed84a67e197cbfd2b83adcb0/verification/public-data-composition/aarch64-unknown-linux-gnu.json). That executable was built with Rust 1.96.1 and the Linux public callers use Rust 1.97.1. The staged binary SHA-256 is `d96dcd109cc623c800ba6ab6440526eba71b1b4e99ede16fd571acd48c60aaa7`.

The [candidate manifest](../../../oddlang/portable/workspace-v7-canonical-public-data.json) emits workspace lock `b50bd2a6896f7f253b4ed6005844f7cc48704dc3663568815df811e60b68f3ab`. [ARM64 macOS receipt](aarch64-apple-darwin.json) · [ARM64 Linux receipt](aarch64-unknown-linux-gnu.json)

Check the four composed manifest and Cargo lock files from the repository root:

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin compose-public-data -- --check
```

Omit `--check` to regenerate the files. Pass `--portable PATH` to use a scratch copy of the portable inputs. The [macOS ARM64 replay](rust-qualification-macos-arm64.json) records their SHA-256 digests and the three drift controls.

The qualification command uses Rust to stage the pinned inputs, isolate the native build and run the external callers. The compiler cache is keyed by framework revision. The gate records the compiler, Cargo artifacts and input revision for each replay.
