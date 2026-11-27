# Command request wire contract

The `workspace-basic-commands.json` candidate includes the five request operations in the generated native package. Run the standalone differential observer with a built Oddlang CLI and a read-only Marmish checkout containing commit `af54339d250290490d62d4475a77539b7e739071`.

```sh
cargo +1.97.1 run --quiet --offline --locked --manifest-path verification/whole/canonical-public-data/Cargo.toml --bin check-response-envelopes -- requests /absolute/path/to/oddlang /absolute/path/to/marmish
```

The generated consumer builds first, with a separate Cargo target and the four portable model/binding files as its projection inputs. The reference consumer compiles the pinned private command codec and its hexadecimal-validation helper. Both process the exact request bytes in `cases.json`, including duplicate keys, trailing whitespace, and inputs that exceed the byte limit. Six accepted requests across the five operations must round-trip byte for byte, including an exactly 32,768-byte submit. The rejection cases compare acceptance only. The observer does not run either daemon.

The receipt used the compiler built from Oddlang revision `1dcf568d7da9aabcdba7dc134eef5654f063096f` with Rust 1.97.1. To check the workspace projection, run `oddlang build native-workspace rust oddlang/portable/workspace-basic-commands.json <new-directory>` and pass `--package <new-directory>` to the observer. It checks the workspace lock and matches the standalone codec to the exact file in the packaged member.
