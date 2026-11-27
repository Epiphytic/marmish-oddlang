# Original and projected retention API

The same Rust caller compiles against the pinned Marmish core crate and the Rust projection of the retention graph. Both runs check the public constructor, accessor, default, upper-bound constant and trait surface. They exercise 14 boundary inputs and 768 deterministic seeded inputs from the full unsigned domain, the accepted range and a neighborhood of the upper boundary. The caller exhaustively matches all seven configuration error variants and checks their exact messages, debug names, ordinal casts and error sources.

The verifier runs the original Cargo retention integration test. It then extracts that exact test function from the pinned file and appends it unchanged to the caller compiled against both implementations. The projection is rebuilt in a fresh directory supplied with the graph and the Rust binding. The original source and library are used to compile and check the reference after the projections have been generated.

The mutation lowers the graph's upper bound by one second while preserving the Rust binding. The caller rejects the result at the original upper bound. That check uses the independent 365-day expectation. The unchanged upstream test derives its upper boundary from the API constant and checks that the API applies that constant consistently.

```sh
cargo +1.96.1 run --quiet --locked --offline --manifest-path verification/whole/core-lexical/Cargo.toml --bin core-value -- \
  /path/to/pinned/marmish /path/to/generated/rust-release /path/to/oddlang /tmp/reference.json
```

[reference.json](reference.json) records the source, graph, generator outputs, caller and observation digests, as well as the TypeScript corpus and generated obligation counts. This receipt covers retention and the public error types. Relay parsing and probe framing have separate observable behavior.
