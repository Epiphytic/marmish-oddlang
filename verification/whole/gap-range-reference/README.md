# Output gap values

The default workspace's Linux import audit identified two unresolved public
engine names, `MissingReason` and `MissingRange`. This independent fixture
examines their data contract with inert JSON. It doesn't start an engine.

The original probe tests 17 reason inputs and 18 range inputs. Both the
string `"Expired"` and the one-entry object `{"Expired":null}` decode to
the same reason and encode as the string. The range record accepts reordered
or unknown object keys and positional three-element arrays. An omitted
`end` re-encodes as null. A reversed numeric interval is admitted. The
receipts preserve the upstream output or the exact JSON diagnostic, line
and column for each fixture.

The [candidate package](../../../oddlang/portable/engine-gap-reason-package.json)
projects the reason enum from a reviewed [domain graph and Rust overlay](../../../oddlang/portable/engine-gap-reason-v1/).
Its four stable case IDs bind their wire spellings independently of native
names. The graph declares clone-only semantics. The candidate uses
the identical reason branch of the original probe, and checks all four native variants, their order, clone equality and Debug
spelling. Both libraries reject a Copy bound with E0277. The range record remains a reference contract.
The candidate is standalone; the canonical workspace has its own pins.

To reproduce, use the clean original at
`af54339d250290490d62d4475a77539b7e739071`, native Rust 1.97.1 and cached
Cargo inputs. Use a clean checkout of Oddlang revision
`3c01918dd430afcf3b57a21a5b24191f99333025` with the Rust 1.96.1 framework toolchain installed. Use a dedicated target directory
outside the original checkout. The first command checks the original and
archives its observations. The second needs the reference receipt, probes
and portable inputs and can run with the original checkout absent.

```sh
host=$(rustc +1.97.1 -vV | sed -n 's/^host: //p')
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin gap-range-reference -- observe "$ORIGINAL" "$TARGET" "/tmp/original-$host.json"
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin gap-range-reference -- project "$FRAMEWORK" "$COMPILER_TARGET" "/tmp/generated-$host.json"
```

A candidate match in this lane covers the tested Serde JSON representations,
native access to the root type and its Copy exclusion. The diagnostic
strings include the dependency version's wording. Stable compiler, input,
package and probe pins are recorded beside the results.

The Rust tools replayed all 35 upstream cases and 17 independently projected reason cases on Apple Silicon, including the native Copy rejection. A previously populated target changed the unknown-field result. Rebuilding in an empty target reproduced the archived range behavior. Use an empty external target for a fresh upstream observation. [Rust replay receipt](rust-replay-aarch64-apple-darwin.json)
