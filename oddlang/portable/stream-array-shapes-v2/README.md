# Stream array dimensions

The [portable graph](shapes.json) records the native compiler’s 186 declarations for the default release stream library. Each array has an element type and an extent. For example, the transfer identifier in `Launch` is an array of 32 unsigned 8-bit elements. The graph uses structural types and field identities throughout.

Compared with the [v1 archive](../workspace-shapes-v1/release.json), all 63 fields previously marked with an array-length gap now have a fixed extent. Replacing each new array shape with the v1 unresolved shape reproduces the entire earlier declaration set. The other structural gaps retain their original owners and reasons.

The [verifier](../../../verification/whole/shapes/arrays/replay.py) also compares the graph with an independent rustdoc projection from the original source. It corroborates all 33 affected fields visible in rustdoc, including locally aliased byte arrays. The other 30 are compiler-generated fields covered by the compiler observation. Increasing a corroborated extent by one makes the comparison fail.

Run on a supported ARM64 host with both pinned toolchains, the 1.97.1 compiler-development component and the pinned dependencies available to Cargo. The upstream and Oddlang checkouts must be clean. Use the framework revision recorded by the [macOS receipt](../../../verification/whole/shapes/arrays/aarch64-apple-darwin.json).

```sh
python3 -B verification/whole/shapes/arrays/replay.py /path/to/original /path/to/oddlang /tmp/new-stream-shapes
```

The graph records the observed structure of the upstream crate. The workspace release manifest selects the behavior graphs used to build binaries.

The [Linux ARM64 receipt](../../../verification/whole/shapes/arrays/aarch64-unknown-linux-gnu.json) and macOS receipt record identical graph bytes, field identities, independent observations and remaining gaps. Per-platform locks record the selected compiler and Cargo context.
