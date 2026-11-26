# Acknowledged stream windows

The graph defines two bounded byte journals. Each operation has typed inputs, ordered decisions, a result and a list of effects that succeed together. Named expressions share checks such as remaining credit and the valid cursor interval. The outbound journal retains data until acknowledgment. The inbound journal keeps bounded digest receipts to recognize exact retries and distinguish conflicts and stale retries after eviction. The separate Rust overlay binds method names and the private storage fields inspected by the upstream Rust tests.

Build from the companion repository with Oddlang at `43d1f301e75db31ce320fe87252002370b48783b`:

```sh
oddlang build native-package rust oddlang/portable/stream-windows-package.json /tmp/stream-windows
cmp oddlang/portable/stream-windows-v1/package.lock /tmp/stream-windows/package.lock
cargo +1.97.1 build --manifest-path /tmp/stream-windows/Cargo.toml --release --offline --locked
```

This package includes shared declarations, the codec module and the windows module. The graph was authored from the observed behavior and reviewed native declarations. The native storage binding permits three original tests to examine the same private observations. The graph and binding do not include upstream function bodies.
