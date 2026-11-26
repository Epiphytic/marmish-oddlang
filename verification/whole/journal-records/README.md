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
