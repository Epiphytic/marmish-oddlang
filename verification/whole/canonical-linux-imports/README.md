# Canonical Linux ARM64 import audit

This compile-only audit tests the frozen Darwin corpus of 140 type, alias
and module import spellings against the pinned original and the generated
canonical workspace on Linux ARM64. The same corpus supports comparison
with the [archived Darwin audit](../canonical-workspace/aarch64-apple-darwin.json).
The prototype contributes zero import spellings. Its library still builds
and passes the downstream crate-alias control, as do the other six
libraries on each side. Each import is compiled separately with
`--emit metadata`.

## Results

| Library | Frozen spellings | Original Linux | Generated Linux | Generated Darwin |
| --- | ---: | ---: | ---: | ---: |
| Core | 6 | 6 | 6 | 6 |
| Engine | 48 | 48 | 7 | 7 |
| MDK | 5 | 5 | 2 | 2 |
| Native | 36 | 36 | 18 | 18 |
| Prototype | 0 | 0 | 0 | 0 |
| Stream | 35 | 35 | 35 | 35 |
| Vault | 10 | 10 | 9 | 9 |
| All | 140 | 140 | 77 | 77 |

Each generated result agrees with its Darwin counterpart. All 63
generated rejections are `E0432` and match by spelling and diagnostic
message across those platforms. The [Linux receipt](aarch64-unknown-linux-gnu.json)
retains every observation, including the compiler's rendered explanation
for each rejected spelling. The original Linux run independently
confirms all six core spellings, and the canonical Linux run compiles
all six against the generated core.

## Reproduce

Use the original at `af54339d250290490d62d4475a77539b7e739071`. Build
the canonical workspace from the retained companion inputs
`0f9e57396530fa1c11127e3fff7d8d0018514f00` with the Oddlang compiler
at `8a20ea283d0d43da42f054a7c7774eb9e6a20da7`. Its generated workspace lock
must hash to `5abf57c811d641ba126c32dde208334d85d47374bbfbb983231783a403f24df4`.
Run in a full companion checkout on native Linux ARM64 with Rust 1.97.1
and the pinned dependencies available offline:

```sh
"$ODDLANG" build native-workspace rust \
  "$CANONICAL_INPUTS/oddlang/portable/workspace-v7-canonical-offline.json" "$PRODUCT"
python3 -B verification/whole/canonical-linux-imports/verify.py \
  "$PRODUCT" "$GENERATED_TARGET" "$CANONICAL_INPUTS" \
  "$ORIGINAL" "$ORIGINAL_TARGET" "$RECEIPT"
```

Use separate Cargo target directories outside both source trees. The
verifier invokes locked, offline Cargo for both exact library workspaces
and accepts Cargo's cache decisions. It selects the actual Linux release
artifact paths from fresh Cargo output and records the two artifact-log
digests, library and metadata hashes, and the hashes of Cargo-reported
library, metadata and proc-macro closure files. It checks that closure
again after probing. Original Git and manifests, generated tree and
package locks, and frozen inventory pins are checked before and after.
Two controls per build require `E0432` for an absent import and `E0603`
for the existing private configuration module.

The audit does not discover Linux-only spellings outside this corpus.
A successful import establishes that rustc resolves the spelling for
this native profile. Type identity, function signatures and runtime
behavior require separate evidence.
