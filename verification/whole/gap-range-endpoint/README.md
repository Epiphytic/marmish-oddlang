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
the expected Rust observations. `native.py` compiles the unchanged wire
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
python3 -B "$lane/native.py" "$ORIGINAL" "$ORIGINAL_TARGET" "$RETAINED" "$REFERENCE_RECEIPT"
python3 -B "$lane/verify.py" "$FRAMEWORK" "$COMPILER_TARGET" "$ORIGINAL" "$BINDING_RECEIPT" --typescript
```

The verifier stages the published hints, alternative representations,
a profile with one hint deleted, and a profile with the hints file absent. In each profile it pins the same
portable wire contract and case bytes, generates the standalone package
with the explicit Rust field requirement, and compares every emitted file.
The compiler runs with original sources and caller sources denied. After
comparing all four native packages, the verifier compiles the unchanged
wire and Rust trait callers against the baseline bytes and replays each
profile's corpus. The TypeScript probe likewise consumes every profile's
corpus. A private-field declaration is rejected for the Rust drop-in
binding. The replacement hint strings are advisory input controls;
they do not describe additional native implementations.
