# Stream rebaseline on Apple Silicon

The original workspace was checked out at `af54339d250290490d62d4475a77539b7e739071`, tree `9be3104dd1d55194ae6e442c0d3eb1c0b223a481`. Its stream package tree `dce608dd897dc4499e23b30423a720e6a1f9a4ce` and workspace Cargo.lock are identical to those at the oracle’s original revision `49fc705408a230bd0ad896248d08dbdd2faad05a`. The receipts distinguish the original binary's source revision and profile from the frozen cases' source revision.

| Run | Result |
| --- | --- |
| Original macOS ARM64 dev and release | Each matched 403 public and 128 sealed stream cases, crossed 31 serialized records, decoded the alternate zlib peer’s bytes and detected three negative controls. The canonical public observation SHA-256 was `00693291df0b7ded5774ff7ba9a508a71b59162e0cdf210cbab4cb29b7e87ff5` in both profiles and the previous release. |
| Release original against generated Rust and TypeScript on macOS ARM64 | Each matched 253 public and 112 sealed Data cases. All six directed writer/reader pairings agreed on 36 records each. Both generated targets changed encoding and decoding when the graph tag changed from 5 to 51 and the original rejected those mutated records. |
| Original upstream stream tests, default features | Rust 1.97.1 ran 29 release tests successfully on macOS ARM64. There were zero doctests. |

The graph recovered from the new pin has the published Data semantic digest `82ddfed871221091be6b42adf6d0b009cb5ce10f864c414bdbaed5e19e4dad56`. Only the Data role is generated in this qualification. The broader corpus runs against the original stream library. Each result row links to its full measurements in [original-dev.json](original-dev.json), [original-release.json](original-release.json) or [data-release.json](data-release.json). The gzip contains [public observations](public.json.gz).

Build the Oddlang CLI in the framework checkout. Supply a clean upstream checkout at the required commit and Cargo’s offline dependency cache. The isolated Data qualification additionally requires the macOS Xcode SDK and Node. Replay into a fresh directory with the framework checkout that contains the original revision and profile options.

```sh
python3 verification/whole/stream/replay.py /path/to/oddlang /path/to/marmish \
  /path/to/oddlang/target/debug/oddlang /tmp/stream-replay
cargo +1.97.1 test --manifest-path /path/to/marmish/Cargo.toml \
  --release --locked --offline --package marmish-stream
```

Qualification owners may pass `--holdout PATH` to include the sealed cases on macOS. The public replay validates the observation digest, mixed-peer digests and mutation outcomes against the archived prior qualification. Native Linux runners can use the same command for the two public original profiles. Linux Data generation and sealed replay still require an independently verified isolation boundary.
