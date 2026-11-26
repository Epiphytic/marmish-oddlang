# Derive the recovered gap package

The [compact package manifest](../../../oddlang/portable/engine-gap-range-derived-package.json)
assembles the recovered `MissingReason` and `MissingRange` graphs into the
same two-type standalone engine library as the
[explicit recovered package](../gap-range-recovery/).

Both rows point to pinned recovery locks. The recovered graph and Rust
binding live beside each lock. The assembler derives component kind,
public re-exports, and graph and overlay pins. The record row also derives
the Rust module. The choice's recovery has no containing module binding, so
the input still supplies `model` for that row. The two recovery identities
and their order, package name, version, Rust edition, Serde dependency,
and Cargo lock remain explicit packaging decisions.

The range lock is a verbatim copy of the archived Darwin baseline compiler
receipt. The graph and Rust binding are the cross-host recovered baseline
inputs selected in the [source-recovered workspace](../canonical-recovered-range/).
The compiler verifies the selected receipt and its graph and overlay
digests. Independent recovery qualification establishes the source
evidence on which this assembly depends.

## Reproduce

Use the pinned original checkout and the framework revision recorded
by the verifier. Build inputs must be available offline under Rust 1.96.1.

```sh
python3 -B verification/whole/gap-range-assembly/verify.py \
  "$FRAMEWORK" "$COMPILER_TARGET" "$ORIGINAL" "$RECEIPT"
```

Only the compact descriptor, recovery locks, graphs, overlays, and Cargo
lock are staged. The sandbox denies access to the original model and
crate root, the earlier explicit package manifest and package lock, and
the native caller. The verifier compares the resolved lock entries and
every emitted package file against the independently sealed reference.
The input manifest digest and added recovery provenance account for the
new lock bytes. All generated Rust, Cargo manifests, and Cargo.lock must
match the earlier independently caller-qualified package. Tampering
with the graph or recovery lock must be rejected before package output.
