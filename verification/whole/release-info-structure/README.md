# Structural release-info recovery

This fixture recovers the ordered Rust release metadata record from the
pinned Marmish checkout. Its selector identifies the record, constructor,
ordinary crate roots and source digests. It also identifies the compile-time
environment names for the build target and represented release revision.
Native output checks establish those bindings for this release.

The portable graph retains declaration identities, source widths, signedness,
alias chains, fixed array length and conversion semantics. In particular the
engine entry refers to the engine crate's public declaration, which aliases the
store declaration. The separate workspace manifest assembles this graph with the
previously qualified command guards. No upstream Rust is a projection input.

The verifier builds both native client and daemon commands from the pinned
original and from their independently assembled workspace. It compares ordered
stdout bytes, successful exit and empty stderr in fresh private process
directories. It then repeats the run after a scratch-only engine constant
mutation from 1 to 4,294,967,305. The whole report changes only at array slot two
(zero-based). Conversion to u32 yields 9. Both builds use the original represented
revision to isolate this change. The receipt records the mutation's actual source
commit separately.
A separate Cargo check in the scratch checkout verifies the selected Rust
primitive types, including the formats array and the public engine alias.


Run on Darwin ARM64 using Rust and Cargo 1.97.1 and an Oddlang binary built at the
revision recorded in the receipt. The offline Cargo dependency cache must be
populated. Use a fresh output directory and reuse separate target roots dedicated to this verifier. The verifier keeps
baseline and mutant Cargo caches in distinct subdirectories.

```sh
cargo +1.97.1 run --locked --offline --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin release-info-structure -- \
  /path/to/oddlang /path/to/original /tmp/recovered-release-info-run \
  --framework-revision FRAMEWORK_SHA \
  --original-target /tmp/recovered-report-original-target \
  --generated-target /tmp/recovered-report-generated-target
```

For a replay using retained Darwin upstream binaries, add
`--retained-original-pins verification/whole/release-info-structure/darwin-original-pins.json`.
The verifier checks SHA-256 for both roles under `baseline/debug` and
`mutant/debug` in the original target root before observing their metadata.
These digests identify the specific retained executables used for this replay.
The generated workspaces and commands, and the scratch source type probe,
still compile during that run.

This phase qualifies the ordinary development-layout report and its named
inputs. It leaves v7's published release pins and prior command coverage in place.

With the four pinned Darwin upstream witnesses, the Rust verifier reproduced
both archived transcripts and the receipt with SHA-256
`99f863e4f34fdc8f3bc92675fa992dc8b3c041eb708f2f2217f47108a06f2065`.
