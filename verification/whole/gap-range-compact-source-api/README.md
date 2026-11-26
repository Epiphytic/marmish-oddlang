# Compact assembly with the checked Rust endpoint

The [compact descriptor](../../../oddlang/portable/engine-gap-range-derived-source-api-package.json)
adds one `sourceApi` pin to the earlier [two-type assembly](../gap-range-assembly/).
It selects the existing [Rust binding](../../../oddlang/portable/engine-gap-range-native-idioms-v1/rust.json)
requiring a public `end: Option<u64>` field. The recovered graph and binding
still determine component kind and exports. Fields outside the separate
source-API assertion must match the recovered binding.

The verifier stages only the compact inputs and checks source denial
before generation. It compares every generated package file to the sealed
[checked endpoint build](../gap-range-endpoint/binding-aarch64-apple-darwin.json)
and compares the prior compact package lock with the new manifest and
source-API provenance accounted for. It compiles the unchanged typed
field caller and replays the unchanged wire probe against the compact
package. The portable contract, advisory hints, and shared cases are the
same pinned inputs from the [endpoint qualification](../gap-range-endpoint/).

The controls reject an altered binding behind a stale digest, a
re-pinned change to the recovered module presentation, and a re-pinned
private field for the public source-API role. Each rejection precedes
output creation.

## Reproduce

Use the exact Oddlang runtime pinned in the verifier, the pinned
original checkout, and cached Cargo inputs. Build Oddlang with
Rust 1.96.1 and run the unchanged callers with Rust 1.97.1 on Darwin
ARM64.

```sh
python3 -B verification/whole/gap-range-compact-source-api/verify.py \
  "$FRAMEWORK" "$COMPILER_TARGET" "$ORIGINAL" "$RECEIPT"
```

The [Darwin receipt](aarch64-apple-darwin.json) records all 11 shared wire
observations, six matching generated file hashes, and the diagnostics
from all three controls. This standalone experiment leaves workspace
selection and prior packages unchanged.
