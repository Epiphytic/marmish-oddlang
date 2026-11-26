# Marmish sources recovered by Oddlang

The standalone [vault value library](../verification/whole/vault-values/README.md) covers pure checked values and metadata. It has its own qualified package manifest.

The [workspace public API comparison](../verification/whole/public-types/README.md#whole-workspace-on-darwin-arm64) covers all seven original libraries and shows which v5 and v6 types downstream Rust code can import.

The [recovered stream data shapes](portable/stream-array-shapes-v2/README.md) resolve 63 field extents previously unknown. Rustdoc independently corroborates 33 of them.

Build the current Rust workspace from the [v7 manifest](portable/workspace-v7.json). It adds `release-info` to both native binaries and preserves the v6 initialization, lifecycle, status, core and stream scope. [Build and qualification](../verification/whole/workspace/README.md#workspace-v7)

To build v5 from the 55 pinned graph, overlay and lock inputs with an empty Cargo cache, follow the [fresh-build instructions](../verification/whole/workspace/README.md#bootstrap-v5-with-an-empty-cargo-cache).

The [copy correspondence verifier](../verification/whole/copy-source-correspondence/README.md) derives `Launch::matches_manifest` from pinned Rust compiler evidence and checks a source mutation against independently compiled native callers.

The [source-derived copy frame profile](../verification/whole/source-copy-frame/README.md) recovers all eleven frame tags and the ordered top-level payload writes from the pinned Rust encoder.

The [source-derived preflight release](../verification/whole/source-copy-frame-guards/README.md) recovers the ordering and error identities of nine encoder guards.

The [checked frame fragment release](../verification/whole/source-copy-frame-fragments/README.md) recovers the Start and Manifest prechecks and the field writes in both fragment encoders.

Oddlang recovered two modules from Marmish commit `49fc705408a230bd0ad896248d08dbdd2faad05a`. The semantic source is in Turtle.

| Oddlang source | Generated Rust |
| --- | --- |
| [Framing graph](framing/domain.ttl) | [frame.rs](../crates/marmish-core/src/frame.rs) |
| [Public façade graph](facade/domain.ttl) | [lib.rs](../crates/marmish-core/src/lib.rs) |

Each graph ships with its import lock and a source map. The TOML files describe recovery from the original Rust checkout. The assembly manifest records both generated modules and the retained paths. The rest of Marmish runs as retained native code. The qualified source snapshot is `e0d855874ec2348939175dfa6ec5c71488c8ba35`. [Qualification and whole-workspace rebuild](https://github.com/Epiphytic/oddlang/blob/292b0bdba562fe311b778fec067b71b545d49fda/docs/design/marmish-qualification.md)

From the root of this clone, use the Oddlang CLI built from commit `292b0bdba562fe311b778fec067b71b545d49fda` of the compiler repository. The pinned release digests bind the exact graph, lock and source map supplied here. Changing a graph requires a new qualified release pin. [Oddlang compiler](https://github.com/Epiphytic/oddlang/tree/292b0bdba562fe311b778fec067b71b545d49fda)

```sh
out=$(mktemp -d)
oddlang build module oddlang/framing "$out/framing" \
  --import-sha256 e39cf8da02d75442a98c1a338f694931ce5b7b2b5cb70c2759c50f1efea8bdcc
oddlang build module oddlang/facade "$out/facade" \
  --import-sha256 8d8e236754261c0b3d0e0899b285e3e0035ce74178e5573e0df01ea938d8d2e1
cmp "$out/framing/frame.rs" crates/marmish-core/src/frame.rs
cmp "$out/facade/lib.rs" crates/marmish-core/src/lib.rs
```

## Regenerate marmish-stream

All five Rust sources for `marmish-stream` live in the pinned
[syntax graph](stream/syntax.ttl). The
[source inventory](stream/sources.json) records their package-relative paths
and output hashes. Its Cargo manifest and the workspace lock remain at their
original paths. Other workspace crates are available as native Rust sources.

Build the Oddlang CLI from the
[pinned compiler](https://github.com/Epiphytic/oddlang/tree/f5f6ea6571821ce49f0622d9550d50ce7c8de8e7)
using `cargo +1.97.1 build --locked --bin oddlang`. Then run these commands
from the Marmish repository. Point `ODDLANG` to the resulting binary when
it is outside your PATH.

```sh
ODDLANG=/path/to/oddlang ./oddlang/regenerate-stream.sh
cargo +1.97.1 test --locked --offline -p marmish-stream
cargo +1.97.1 check --locked --offline --workspace --all-targets
```

The script verifies the graph and every emitted source against
[the lock pin](stream.sha256). A repeat run checks the materialized sources.
Cargo uses the existing crate name, path, version, API and locked dependencies.
For offline builds, prepare the dependencies in the local Cargo cache first.
The input to regeneration is this repository's Oddlang graph bundle.

## Portable Data contract

The [copy Data role](portable/copy-data-v1/) supplies a contract graph and
pin for standalone Rust and TypeScript modules. It covers the Data record
and bounded incremental decoding observed at the pinned upstream revision.
