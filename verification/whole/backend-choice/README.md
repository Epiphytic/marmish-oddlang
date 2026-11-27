# Native BackendChoice

This standalone package exposes the three fieldless cases of
`marmish_native::identity::BackendChoice` from a pinned portable graph. It
captures declaration order and case equality. Its Rust projection implements
Clone, Copy, PartialEq and Eq. The package contains only the selected type.

One unchanged downstream Rust caller compares the full equality matrix and
Rust discriminants, exhaustive matching, cloning/copying, and casts to
`isize`, `u8` and `i128`. Separate negative compilations check Debug,
Display, Default, Hash, Ord, PartialOrd, Serialize, Deserialize, an incomplete
match, integer conversions, cross-type equality and a crate-root import. The runner uses the actual original Serde
dependency for the trait checks in both lanes.

A scratch commit rotates the original enum cases from `Auto, Keychain, File`
to `File, Auto, Keychain`. The recovery follows the changed Rust order. The
original and generated libraries agree on the new cast values and on all
compile-time checks. No provider operations are exercised. The ordinary
module-level type is the scope of the comparison.

Run on native Darwin ARM64 with Rust and Cargo 1.97.1, a clean original
checkout at the receipt's revision, and the locked dependencies available
offline. The runner separates baseline and mutant caches below each target
root and snapshots the compiled libraries.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin backend-choice -- qualify \
  /path/to/oddlang /path/to/original /tmp/backend-choice-replay \
  --framework-revision FRAMEWORK_SHA \
  --original-target /tmp/backend-choice-upstream-targets \
  --generated-target /tmp/backend-choice-candidate-targets
```
