# Engine scalar and native path records

One sealed seven-member workspace adds the public engine records
`InputReceipt` and `MaintenanceReport`, and the native
`transport::ArtiDirs` record. The root Cargo files and sibling edges are
identical to the converged prototype constant, engine error and identity
workspace. Only the engine and native package rows change.

The independent callers verify actual construction and field access,
integer widths, boundary values, exact Debug, independent Clone values,
and fieldwise default. The path caller uses relative paths and native
non-UTF-8 bytes. The original and generated outputs match, including
32 negative Rust compile checks for undeclared capabilities, widths and
export paths. The macOS source mutation reorders actual
`MaintenanceReport` fields and observes the reordered Debug output in
both the compiled upstream and the generated record.

## Other public data

The original source and published import audit identify several groups
whose members share the same prerequisites:

| Data family | Representative original declarations | Reusable capabilities |
| --- | --- | --- |
| Scalar and owned-path records | `InputReceipt`, `MaintenanceReport`, `transport::ArtiDirs` | Ordered named fields, u32/u64, booleans, owned paths, Clone, Debug, fieldwise Default |
| Fieldless enum values | `Stream`, `SubmitDisposition` | Closed declaration-order cases with exact Clone, Copy, Debug and equality selection |
| Fieldless values with Serde | `InvocationState`, `InputHealth`, `LaunchFailure`, `MissingReason` | Exact trait sets and declared serialization of cases |
| Public identifier wrappers | `PrincipalId`, `EndpointId`, `ServerId`, `InvocationId`, `JobId`, `InputGeneration` | Compiler-observed public tuple fields of exact byte extent, Clone, Copy, Debug, equality, Hash and Serde |
| Nested observation records | `MissingRange`, `OutputStatus`, `OutputChunk`, `InputStatus`, `InvocationStatus`, `Submission` | Typed record references, options, sequences and the dependent identifier and choice contracts |
| Payload enums | `ExitOutcome`, `LiveSpawnError` | Named typed dependencies, signed-width and enum tuple payloads |
| Native counts and configuration data | `mdk_adapter::RuntimeStatus`, `transport::TransportLimits`, `EngineLimits` | Platform-sized counts or durations, named dependencies, and explicit nonzero defaults where declared |
| Process observation records | `LiveProcessRecord` | Fixed byte extent, signed-width fields and explicit Serde admission |

This batch qualifies the scalar and owned-path records. The other rows
identify later data capabilities. Provider and live handle types require
separate effect or ownership evidence.

## Replay

Build and compare both native ARM64 releases from a clean checkout:

```sh
python3 -B verification/whole/engine-native-records/verify.py \
  "$ODDLANG_CHECKOUT" "$MARMISH_CHECKOUT" "$RECEIPT" \
  --framework-revision 6457c87f65f542d47045d0338a64e99346e1ca39 \
  --compiler-target "$ODDLANG_TARGET" --original-target "$MARMISH_TARGET" \
  --original-bin-dir "$MARMISH_BIN_DIR"
```

The native runs pin original `af54339d250290490d62d4475a77539b7e739071`
and companion input `d27cc22f1bc3891a0d7c7f765ce209f616c343c0`.
The original native release binaries and locked dependencies must be
available offline. The separate `mutate.py` takes the built Oddlang CLI,
the pinned original, an independent Cargo target directory, a receipt
output path and the framework revision.
