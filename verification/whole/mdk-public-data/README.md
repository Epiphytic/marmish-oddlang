# Pure public MDK data

The standalone source-free `marmish-mdk` package exposes `AdapterError` and
`VersionObservation` at its crate root. Their native projections use extracted
graphs and Rust bindings pinned by the package manifest. This phase covers
the two pure types on native Unix.

An unchanged Rust caller compares every error variant, declaration ordinal,
exact Display and Debug text, and the empty standard error source. It writes
and reads the two public String fields and observes the record's derived Debug
format. Separate compile failures cover the root Result alias and unsupported
traits on the selected types.

A scratch commit changes the response diagnostic and swaps the source
record's two field declarations. Native original and generated consumers
report the same two changes. The recorded scope is the pure data API. The
caller exercises no adapter methods.

Run on native Darwin ARM64 with Rust and Cargo 1.97.1, the pinned original at
the receipt revision, and the locked Cargo dependencies available offline.
Use a fresh output directory and separate target roots. The verifier isolates
baseline and mutant artifacts underneath each root.

```sh
RUSTUP_TOOLCHAIN=1.97.1 python3 -B verification/whole/mdk-public-data/verify.py \
  /path/to/oddlang /path/to/original /tmp/mdk-public-data-run \
  --framework-revision FRAMEWORK_SHA \
  --original-target /tmp/mdk-original-targets \
  --generated-target /tmp/mdk-generated-targets
```
