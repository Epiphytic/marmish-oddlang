# Seven-member offline CLI observations

The two Darwin ARM64 original binaries are pinned by the existing
[seven-member release receipt](../unified-buffered/aarch64-apple-darwin.json).
The original and generated artifacts matched all 68 archived root and
subcommand help, version and unknown-option observations. The supplemental
matrix exercises required arguments, option relationships, enumerated values,
numeric bounds, abbreviated help/version forms and the auxiliary parser.

| Executable | Supplemental v7 matches | Successor matches | Cases |
| --- | ---: | ---: | ---: |
| `marmish` | 26 | 30 | 32 |
| `marmishd` | 26 | 30 | 32 |

The four recovered cases per binary are direct `rsh --help`, `rsh -h`,
`rsh` with its required argument missing, and `rsh --idle-seconds=0`.
The original's root `help rsh` already matched and remains a description of
the root command grammar. Direct selection uses the recovered auxiliary
declaration, including its independent display name, options and variadic
positional. The successor leaves the selector unbound, including when root
parsing recognizes it after global arguments. Clap renders the help and usage from those facts. Archived
observations are verifier inputs only.

The two remaining differences per binary involve options before `rsh`.
The original prints auxiliary help with `--debug rsh --help`. It rejects
`--user=audit rsh --help` with exit 255 and its pre-parser diagnostic.
The successor prints root subcommand help for both. Original custom
separator checks, other pre-parser rules, successful auxiliary parsing and
operational behavior remain uncovered. The invocation corpus supplies no
remote host or command. The process sandbox denies network, filesystem
writes and forks, and the test gives each executable an empty home and stdin.

## Reproduce on Darwin ARM64

Build Oddlang from the baseline and successor revisions in the build
receipt using Rust 1.97.1 and `cargo build --release --locked --offline`.
Set `BASE_COMPILER` and `NEXT_COMPILER` to their executables, and
`BASE_FRAMEWORK` and `NEXT_FRAMEWORK` to their respective checkouts.
Set `ORIGINAL` to the pinned upstream checkout and `ORIGINAL_BIN` to
the original binary directory matching the release receipt. Create an
empty scratch directory and set `WORK` to its absolute path.

```sh
tools=verification/whole/canonical-public-data/Cargo.toml
cargo +1.97.1 run --locked --offline --manifest-path "$tools" \
  --bin workspace-product -- compose-cli offline-cli "$NEXT_COMPILER" "$ORIGINAL" oddlang/portable
cargo +1.97.1 run --locked --offline --manifest-path "$tools" \
  --bin build-offline-cli -- "$BASE_COMPILER" "$BASE_FRAMEWORK" "$ORIGINAL" \
  "$WORK/base-product" "$WORK/base-cargo" "$WORK/base-build.json"
cargo +1.97.1 run --locked --offline --manifest-path "$tools" \
  --bin build-offline-cli -- "$NEXT_COMPILER" "$NEXT_FRAMEWORK" "$ORIGINAL" \
  "$WORK/next-product" "$WORK/next-cargo" "$WORK/next-build.json" --successor
cargo +1.97.1 run --locked --offline --manifest-path "$tools" \
  --bin workspace-product -- compare-offline-cli "$ORIGINAL_BIN" "$WORK/base-cargo/release" \
  "$WORK/cli.json" "$WORK/base-build.json" "$WORK/next-cargo/release" "$WORK/next-build.json"
```

Composition recovers the auxiliary declaration from the pinned Git object and
checks the authored direct selection and resulting graph against their
published bytes. Generation takes the staged graph closure, with framework,
original and companion source inaccessible and network denied. The regenerated
workspace and package trees are checked against the sealed locks before and
after the release build. The successor preserves all six other member records
and the workspace Cargo dependency lock. The audit verifies both generated
binary hashes against their build reports before invoking either binary.
Binary hashes can vary between build roots. Compare the recorded case rows
and lock hashes for an independent replay.
