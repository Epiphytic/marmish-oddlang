# Independent release declaration observations

Two fresh checkouts of `af54339d250290490d62d4475a77539b7e739071` produced byte-identical reports, SHA-256 `99388bd626cf0862bdb3e9bcc5277239c9e9d260d9fe26ec9bd0b0ee44552149` before gzip. Both observers used Rust 1.97.1, compiler commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, release, default features and the macOS ARM64 target. Rustdoc’s implicit `cfg(doc)` is recorded. The report contains portable paths and declaration witnesses. It includes no function bodies. [Comparison](release-comparison.json.gz)

The ownership-path comparison matched 359 unique declaration paths and kinds. Two additional compiler paths identify block-local items, `wire::deserialize_hex::nibble` and `wire::serialize_hex::HEX`; their exact ownership appears in the report. Three repeated anonymous-constant keys collide. The observations still lack ownership keys for 2,901 compiler declarations and 888 rustdoc declarations, including impl members and synthesized declarations. This prevents an exact-structure claim.

Matching toolchains and profile settings did not remove the historical span differences. The older method still has 373 compiler-only and 72 rustdoc-only keys when it includes expansions. All 445 retain the same side. The report records each key and the result from both runs. Source spans identify witnesses and do not establish semantic identity. [Historical input](historical-spans.json.gz)

```sh
gzip -dc verification/whole/stream/structure/historical-spans.json.gz >/tmp/old-spans.json
cargo +1.96.1 run --locked --offline --manifest-path /path/to/oddlang/verification/provider-validation/Cargo.toml --bin release-rustdoc -- \
  /path/to/pinned/marmish marmish-stream lib marmish_stream aarch64-apple-darwin \
  --revision af54339d250290490d62d4475a77539b7e739071 --previous-spans /tmp/old-spans.json --out /tmp/release-comparison.json
```
