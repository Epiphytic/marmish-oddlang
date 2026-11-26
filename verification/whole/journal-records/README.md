# Public native journal records

This slice recovers `Conversation`, `SendStatus`, and `ReceivedMessage` from
the pinned original `marmish-native` journal. The [source selection](../../../oddlang/portable/native-journal-records-v1/selection.json)
pins the native package Cargo manifest and its explicit Rust 2024 edition,
plus the crate module declarations, both Rust modules, and four local
child scopes used by the original journal glob imports. The [portable graph](../../../oddlang/portable/native-journal-records-v1/graph.json)
contains thirteen ordered fields and three public re-export relations.
The Rust overlay supplies the public names used under `journal` and `mdk_adapter`.

An independent caller constructs the values through `journal`, assigns them
in both directions across the two import paths, checks field types, proves
clone independence and serializes each value. A source mutation swaps the
first two declared fields of `ReceivedMessage`. The original and the product
recovered from the mutated graph both emit the resulting new JSON order.

The combined verifier generates the five-member vault, engine, core, stream
and native workspace from source-denied, network-denied input.
It compiles both ordinary native binaries and links the record caller
to the generated native library. Use `--standalone` to run the same record
qualification against the thin native library package.

```sh
python3 -B verification/whole/journal-records/verify.py \
    PATH_TO_ODDLANG PATH_TO_PINNED_ORIGINAL /tmp/native-journal-records.json \
    --original-target PATH_TO_LOCAL_ORIGINAL_CARGO_TARGET
```

Use Oddlang commit `8176d00c0aba824084078b981f89a39947aee629`, Rust 1.97.1, and
the [workspace build prerequisites](../workspace/README.md). The original
version is `af54339d250290490d62d4475a77539b7e739071`. The generated inputs,
including the mutation, contain graph data, Rust identifier bindings and
Cargo locks.

The scope is public record construction, field shape, clone semantics,
serialization order and alias assignability. Journal storage, transport,
commands and MDK behavior remain separate obligations.

## Composition and receipts

The macOS ARM64 replay extends [companion #67](https://github.com/Epiphytic/marmish-oddlang/pull/67)
at `aee87208cdfe403a77e652902f65d5b77804f42c`. Its common framework implementation
ancestor is `7ed1c29acde2cc4cfdc58c2489935e703470ae70`. The journal addition changes
only the native member's projected sources. The combined dependency lock,
native workspace edges and four other member receipts match that ancestor.

The recovered build report and fixed-width vault additions in
[companion #70](https://github.com/Epiphytic/marmish-oddlang/pull/70) have their own
qualification and remain outside this replay. Linux ARM64 exercises the
standalone public record package. The reordered source mutation exercises
the standalone package on both hosts.

[macOS ARM64 combined workspace receipt](aarch64-apple-darwin.json)

[Linux ARM64 standalone receipt](aarch64-unknown-linux-gnu.json) ·
[Follow-up source validation](source-validation.json)

Both native receipts were produced by compiler `8176d00c0aba824084078b981f89a39947aee629`.
Follow-up compiler `0a44ffd4355ecd6a12bd8227626786562cea7991` checks Rust
binding identities for raw names and grouped self imports. Its replays of
the pinned original and reorder recover identical locks. Its baseline
projection reproduces the Rust source hash sealed in the native package.
