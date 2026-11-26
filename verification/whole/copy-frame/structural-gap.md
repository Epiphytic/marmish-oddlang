# Copy protocol structural reconciliation

This surface covers the pinned `marmish_stream::copy_protocol` module. The [native probe](exact.rs) compiles its public constants and signatures against both libraries. The unchanged upstream tests and previous copy receipts exercise public record fields, enum shapes and conversions. The Frame case tuple versus named shape, field order and byte tags are declared in the copy Frame graph.

| Public declarations | Evidence |
| --- | --- |
| `VERSION`, `MAX_PATH`, `MAX_CONTROL`, `DURABLE_WINDOW`, `CHECKPOINT_INTERVAL`, `MIN_RETENTION_SECS`, `MAX_RETENTION_SECS`, `CONTEXT_VERSION`, `MAX_CONTEXT`, `HELPER_ENTRYPOINT` | Native type and value assertions |
| `Direction`, `AttemptMode`, `Phase`, `Stage`, `CopyError` | [Copy Launch](../copy-launch/README.md), including all 256 conversion bytes per enum |
| `Launch` | [JSON shape](../copy-launch/README.md), [digest and namespace](../copy-launch-hash/README.md), [Manifest matching](../copy-field-relations/README.md), typed Frame Start payload |
| `HelperContext` | [Value shape and checked codec](../copy-helper-context/README.md), [start and build checks](../copy-field-relations/README.md) |
| `Manifest`, `Mtime` | [Binary metadata](../copy-manifest/README.md), full standalone corpus repeated on the Frame package, typed Frame Manifest payload |
| `Checkpoint`, `FileIdentity` | [Checked composite record](../copy-checkpoint/README.md), full standalone corpus repeated on the Frame package |
| `Frame`, `FrameDecoder` | All 11 constructors and tagged cases, incremental reassembly, default, exact error and poison tests |
| Module's imported `Codec`, `Error`, `Result`, `MAX_BLOCK`, `MAX_OFFSET` | Both consumer crates bind the preexisting stream types and constants by their public owning module paths |

The remaining representation differences are private. This list records the source declarations that have no retained one-to-one native declaration.

- `numbered_enum!` expands into graph-generated numeric and string enum implementations. `CONTEXT_DOMAIN` is checksum/domain graph data. The test harness supplies its exact private value when compiling the unchanged original test module.
- `build_atom`, `read_build_atom`, `namespace`, `validate_path`, `offset`, `codec_number` and `read_codec` are expressed as typed schema rules and inline native bindings.
- `put_u32`, `put_u64`, `put_bytes`, `put_option`, `put_identity`, `put_offset_hash`, `encode_launch`, `encode_manifest`, `read_launch` and `read_manifest` lower from the ordered record layouts and fragment entrypoints.
- `Reader` and its private cursor methods lower to local bounded reads. The `path_encoding` module is a generated scalar byte adapter.
- `FrameDecoder` private storage and `feed_inner` remain implementation details. The witness checks the public layout-independent state transitions, including poisoned feeds and finish behavior.
- The original test module is supplied as immutable verification input after the source-isolated build. Its private fixture helpers stay in that test module.

Compiler-expanded private AST, source coordinates, and Rust ABI memory layout equivalence were outside the qualification boundary. The receipt is a macOS ARM64 library result. Linux, installed transport and filesystem effects require separate evidence.
