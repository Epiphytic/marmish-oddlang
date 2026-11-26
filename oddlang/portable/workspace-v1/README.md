# Build the native workspace

This release composes the core library and the recovered stream modules into one Cargo workspace. Start in the companion repository with Oddlang at the framework revision in the [release receipt](../../../verification/whole/workspace/reference-aarch64-apple-darwin.json). Cargo needs Rust 1.97.1 and a populated cache of the pinned third-party dependencies for offline builds.

```sh
oddlang build native-workspace rust oddlang/portable/workspace.json /tmp/marmish-workspace
cmp oddlang/portable/workspace-v1/workspace.lock /tmp/marmish-workspace/workspace.lock
cargo +1.97.1 build --manifest-path /tmp/marmish-workspace/Cargo.toml --workspace --release --locked --offline
```

The resulting workspace exposes the six public core types and the stream crate’s shared declarations, codec and acknowledged windows. Each member records the semantic inputs and native projection for its modules. One root Cargo.lock pins the combined dependency resolution. The source-free release build precedes test insertion. The verifier then runs seven unchanged core tests and ten unchanged stream module tests inside the generated workspace. The staged tests exercise native private storage as well as public APIs.

The CLI and daemon executable roles, remaining stream modules and other native packages have separate open qualification gates.
