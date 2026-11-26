# Marmish sources recovered by Oddlang

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
