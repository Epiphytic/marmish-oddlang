# MissingRange endpoint

The recovered endpoint is `urn:marmish:engine:output-gap:range:end`.
The [portable wire contract](../../../oddlang/portable/engine-gap-range-native-idioms-v1/wire.json)
declares the `json-wire` role, links the recovered field, and pins the
[shared cases](cases.json). Its paths are relative to the companion
repository root. Omission
and JSON null decode to an open end. An open end encodes as null. A present
endpoint carries an exact unsigned 64-bit integer. The shared object-form
cases exercise `9007199254740993` and `18446744073709551615` as unquoted
JSON numbers, along with rejected inputs.

The [project hints](../../../oddlang/portable/engine-gap-range-native-idioms-v1/hints.json)
attach `Option<u64>` and `bigint | null` to the same endpoint ID for native
Rust and TypeScript consumers of the wire role. The separate
[Rust target binding](../../../oddlang/portable/engine-gap-range-native-idioms-v1/rust.json)
requires the public `end: Option<u64>` field for the drop-in source API.
The existing caller constructs the struct and reads the field into a typed
`Option<u64>` local. See [the native idioms specification](https://github.com/Epiphytic/oddlang/blob/d63dde7/docs/design/contract-recovery.md#native-idioms-and-role-specific-requirements).

The original and the retained workspace build independently establish
the expected Rust observations. `qualify-range-endpoint` compiles the unchanged wire
probe against both. The retained build has the same generated Rust bytes
as the [source-recovered workspace](../canonical-recovered-range/).
The [TypeScript consumer](typescript/) checks the same raw input and
output strings for the endpoint wire role.

## Replay the binding and hints

Use the pinned original checkout, the framework runtime recorded in the
receipt, Rust 1.96.1 to build Oddlang, native Rust 1.97.1 for callers, and cached
Cargo inputs. Run the TypeScript
check with Node 22 and cached TypeScript 5.9.3 by adding `--typescript`.

```sh
lane=verification/whole/gap-range-endpoint
cargo +1.97.1 run --quiet --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin qualify-range-endpoint -- "$ORIGINAL" "$ORIGINAL_TARGET" "$RETAINED" "$REFERENCE_RECEIPT"
python3 -B "$lane/verify.py" "$FRAMEWORK" "$COMPILER_TARGET" "$ORIGINAL" "$BINDING_RECEIPT" --typescript
```

The verifier stages the published hints, alternative representations,
profiles deleting each hint separately, and a profile with the hints file absent. In each profile it pins the same
portable wire contract and case bytes, generates the standalone package
with the explicit Rust field requirement, and compares every emitted file.
The compiler runs with original sources and caller sources denied. After
comparing all five native packages, the verifier compiles the unchanged
wire and Rust trait callers against the baseline bytes and replays each
profile's corpus. The TypeScript probe likewise consumes every profile's
corpus. A private-field declaration is rejected for the Rust drop-in
binding. The replacement hint strings are advisory input controls;
they do not describe additional native implementations.

## Evidence

- The independent native corpus matched all 11 cases on
  [Darwin ARM64](aarch64-apple-darwin.json) and [Linux ARM64](aarch64-unknown-linux-gnu.json).
- The [Darwin binding replay](binding-aarch64-apple-darwin.json) matched all
  11 Rust and TypeScript cases for each hint profile and compiled the unchanged
  Rust source-API caller. A second [Darwin replay](omitted-hints-aarch64-apple-darwin.json)
  ran the same entry point in a checkout with the hints file deleted. Both
  emitted the same package files and matched the same wire outcomes.
- The [seven-member workspace candidate](../gap-range-role-workspace/)
  selects the same Rust role binding.

The [Linux ARM64 binding replay](binding-aarch64-unknown-linux-gnu.json) used a
fresh native ARM64 guest, Rust 1.96.1 to build the framework, Rust 1.97.1 for the
callers, and offline locked Cargo inputs. It compiled the unchanged typed Rust
caller and replayed all 11 cases for each of the five hint profiles. Every
emitted package file matched the corresponding Darwin output. This replay
covers the standalone Rust binding. The Linux TypeScript and sealed
seven-member workspace runs remain outside this evidence.
