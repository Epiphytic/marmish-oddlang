# Pure native release-manifest parser

This standalone native package projects the pinned release manifest's
pure parser and name formatting. It generates crate-visible `Manifest`
and `Asset`, with the original ordered public fields, integer widths,
fixed byte digest, state array, vector item identity and four derives.
Its `Manifest::parse` takes bytes and returns a typed result. The two
formatters construct the release identity and download asset name.
The standalone error domain contains the original stable manifest error
and its display value.

The portable graph declares ASCII framing, twelve exact line shapes,
canonical decimal and lowercase hex captures, the independently recovered
release-version grammar, a finite signed-platform relation, per-asset byte
ceilings and checked aggregate addition. The Rust overlay preserves the
internal native names, widths and visibility. Rejections expose the same
single code, so fine-grained validation precedence has no observable
distinction in this scope. The authored grammar has one compiler-observed
data shape and is verified against behavior. Installation, file discovery,
asset verification, and executable operations are separate boundaries.

The replay compiles the generated package from its graph-only closure inside
the network-isolated build gate. It admits the unchanged native test
caller only afterward. Rustdoc observes both selected declarations from the
actual pinned original and the generated crate. The same native caller
replays all independently published blind inputs. The second graph raises
the license cap by one byte, matching the actual pinned changed-source
oracle. Exactly one case admits a new manifest, while the aggregate-over
case still rejects. The archived original receipts derive from real
original and modified Rust. The second graph instantiates the observed cap
policy, with records and output naming shared across both cases.

To replay, supply the tested [Oddlang compiler revision](https://github.com/Epiphytic/oddlang/tree/5bd0d54858380efadfdfa62d37d8293594ab2ebd),
the original source checkout, and writable Cargo target caches for both
repositories.
Rust 1.97.1 and cached offline dependencies are required.

```sh
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/release-manifest-projection/Cargo.toml -- \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision 5bd0d54858380efadfdfa62d37d8293594ab2ebd \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET"
```

The inherited oracle and portable cases are described in
[original release manifest observations](../release-manifest/README.md).

Build the standalone library from its pinned portable closure.

```sh
oddlang build native-package rust oddlang/portable/native-release-manifest-package.json "$OUTPUT"
cargo +1.97.1 build --manifest-path "$OUTPUT/Cargo.toml" --locked --offline --release
```

Both [ARM64 macOS](aarch64-apple-darwin.json) and
[ARM64 Linux](aarch64-unknown-linux-gnu.json) matched 224 original rows
with 34 baseline acceptances. The changed input matched all 224
changed-source rows with 35 acceptances. The two generated result streams
have identical bytes across the hosts.
