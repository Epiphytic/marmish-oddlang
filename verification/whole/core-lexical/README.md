# Core declaration paths and public import spellings

On each ARM64 host, two observers agreed on 29 unambiguous declaration
keys in the original `marmish-core` and 89 in the generated standalone
core member. The compiler adapter and private-item Rustdoc JSON ran with
Rust 1.97.1 and the default release profile. For each build, the compiler reported two
anonymous constants under one ambiguous key, with zero Rustdoc declarations
for that key. The declared-access scope covers keyed module-level
declarations and struct or union fields, including positional fields. The two observers agreed
on 17 original and 53 generated declarations, with zero differences or
unresolved scopes in either build.

Three unambiguous keys have the same normalized kind and lexical path
across builds. They are the crate and the `config` and `frame` modules.
The [comparison](comparison.json) records how the six original data types
move under additional generated projection modules. It identifies each
native root re-export, its sealed source file, and its component's graph
and overlay inputs. The paths identify local Rust owners and the digest
pins identify portable component inputs.

The [pinned core manifest](https://github.com/Epiphytic/marmish-oddlang/blob/036349fc69ab5b850df2006c16ec2518be95a254/oddlang/portable/core-package.json)
selects those components. The archived [standalone lock](package.lock)
seals the [crate root](export-facade/crate-root.rs.txt),
[config module](export-facade/config.rs.txt) and
[frame module](export-facade/frame.rs.txt) that supply the public
re-exports. These files are verbatim generated evidence.

The independent [canonical Darwin audit](../canonical-workspace/aarch64-apple-darwin.json)
compiled all six downstream crate-root import spellings. That audit used
the full workspace. The lexical observations used its stand-alone core
member and its separate Cargo lock. The Linux replay confirms the
declaration comparisons and provenance. The later [native Linux compile-only audit](../canonical-linux-imports/)
independently established all six frozen core import spellings in both
the original and generated libraries. These observations cover specific
declaration keys, declared visibility and downstream imports.
Type compatibility, effective visibility beyond those import spellings,
behavior, and the completeness of the portable ontology require their
own evidence.

## Reproduce

Use the companion core input from `036349fc69ab5b850df2006c16ec2518be95a254`.
Check out the original at `af54339d250290490d62d4475a77539b7e739071`
and the Rust observer at `b832a8cd9414f15a2c698761029cb76044da9a8f`. The replay also accepts the original Python observer at `d32e179075a660fffa0d3f0905bbe8ae8957c570`, the revision used by the archived receipts.
Build Oddlang at `8a20ea283d0d43da42f054a7c7774eb9e6a20da7` with
`cargo +1.96.1 build --locked --offline --bin oddlang`. Install the native
Rust 1.97.1 toolchain and its rustc development component for the observer.
Run with cached Cargo inputs on the native host identified by the triple.

```sh
"$ODDLANG" build native-package rust oddlang/portable/core-package.json "$CANDIDATE"
python3 -B verification/whole/core-lexical/replay.py \
  "$OBSERVER_CHECKOUT" "$ORIGINAL_CHECKOUT" "$CANDIDATE" "$HOST_TRIPLE" "$OUTPUT"
python3 -B verification/whole/core-lexical/compare.py
```

The replay checks the canonical workspace and member manifest hashes,
component inputs, standalone package lock, Cargo files and every generated
source before and after observation. It also requires a changed source file
in a disposable copy to fail verification under the retained lock. The raw
reports and both provenance snapshots are archived for
[Darwin](aarch64-apple-darwin/) and [Linux](aarch64-unknown-linux-gnu/).
The grouped cross-host comparison recomputes from those archived reports.

The pinned Rust observer reproduced both macOS ARM64 reports byte-for-byte. The generated package had 89 matched lexical keys and the original had 29. Each had one ambiguous key. Replaying the historical observer also reproduced its full archived receipt, including the original producer revision.
