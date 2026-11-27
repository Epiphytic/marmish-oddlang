# Combined recovered report, vault values and journal records

This five-member release extends the recovered report and fixed-width vault
[workspace from companion #70](https://github.com/Epiphytic/marmish-oddlang/pull/70)
and adds the public journal component from
[companion #71](https://github.com/Epiphytic/marmish-oddlang/pull/71).
The assembled Cargo lock, native dependency edges and core, stream, engine
and vault member locks match #70.

The macOS ARM64 gate generates and compiles the entire workspace once.
It uses the resulting two executable files to run the 32 native
`release-info` cases and links the vault and journal probes to the compiled
libraries from that same build. The vault observations include the
unchanged strict value test, array-constructor ownership and the
public value cases. The journal caller checks both import paths, field
types, clone independence and the original serialized order.
Source recovery separately reproduces the source-derived report and the
baseline and reordered journal graphs.

```sh
cargo +1.97.1 run --offline --locked --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin qualify-five-member -- \
  PATH_TO_ODDLANG PATH_TO_ORIGINAL /tmp/recovered-vault-journal.json \
  --original-target PATH_TO_ORIGINAL_CARGO_TARGET \
  --original-bin-dir PATH_TO_ORIGINAL_RELEASE_BINS
```

Use clean Oddlang `84b173b288ef41b60c87aa45291f3f20ccb44bed` and original
`af54339d250290490d62d4475a77539b7e739071` checkouts with Rust 1.97.1.
The [workspace build prerequisites](../workspace/README.md) cover the
toolchain and offline Cargo providers. The generation and release-build
sandbox denies the network, original checkout, source recovery and test
oracles. The only Git inputs are the four locked third-party provider
commits.

The [native receipt](aarch64-apple-darwin.json) records 32 release cases,
600 vault value observations, the strict vault test and constructor
ownership probe, and three journal record outputs. The reference binary
digests match the earlier sealed #70 observation. The receipt binds both
generated binaries and the generated journal and vault libraries to the
same sealed workspace build.
