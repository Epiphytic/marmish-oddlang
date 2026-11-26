# Copy Data role

This release describes the Data record in Marmish's copy protocol and its
bounded incremental decoding behavior. The pinned graph in `contract.ttl`
defines tag 5, the offset and payload layout, length and offset bounds,
and error precedence. It derives from upstream commit
`49fc705408a230bd0ad896248d08dbdd2faad05a`.

Build the Oddlang CLI with the tagged-wire commands. Run the following
from this directory with that executable on the path.

```sh
pin=82ddfed871221091be6b42adf6d0b009cb5ce10f864c414bdbaed5e19e4dad56
out=$(mktemp -d)
oddlang build tagged-wire rust contract.ttl "$out/rust" --graph-sha256 "$pin"
oddlang build tagged-wire typescript contract.ttl "$out/node" --graph-sha256 "$pin"
rustc --edition=2024 --crate-type lib "$out/rust/wire.rs" --out-dir "$out/rust"
node --experimental-strip-types "$out/node/wire.mts"
```

These commands need the contract graph, the pinned digest and native tools.
The advisory hints record observed Rust library choices separately.
[Oddlang command and evidence](https://github.com/Epiphytic/oddlang/tree/feat/portable-rust-recovery/docs/design)

The compatibility evidence applies to the Data wire and decoder role on
the measured profile. The original crate's Rust API, the CLI and the
remaining stream and workspace behavior have separate obligations.
