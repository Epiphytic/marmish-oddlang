# Native RuntimeStatus data qualification

This successor of the canonical seven-member workspace adds `marmish_native::mdk_adapter::RuntimeStatus` to the existing journal record component. The portable graph preserves field order, target pointer-width counter types and the sole declared `Serialize` capability. The qualification uses external Rust constructors and Serde observations on ARM64 macOS and Linux. Each platform checks the original library with its default features and with `installed-checks` enabled. The generated library exposes the same data under the adapter module. The feature replay concerns this type's surface; installed operations are outside this qualification.

Three observations cover zero, escaped and Unicode text, and the native maximum and nearby values. Rust compiler controls check the public import path, distinguish `usize` from `u64`, and detect changes to selected absent traits. A genuine source edit reorders `conversations` and `received` in a separate native checkout. Native serialization follows that order in both the recompiled original and the projection recovered from the changed source.

The recovery lock records the unresolved dependency glob in the original module. Independent Rust callers check actual selected types and capabilities. The sealed package and mutation-projection generators read graphs and overlays with original source and the caller unavailable. The original crate is compiled for comparison, and the callers never initialize or invoke a runtime service.

Reproduce from clean pinned checkouts with Rust 1.97.1 and the offline Cargo dependency cache.

```sh
python3 -B verification/whole/runtime-status-data/verify.py FRAMEWORK ORIGINAL RECEIPT \
  --framework-revision FRAMEWORK_REVISION --compiler-target COMPILER_TARGET \
  --original-target ORIGINAL_TARGET
```

The previous workspace remains an independent build pin. The successor retains its Cargo lock, workspace dependency edges and six sibling locks.
