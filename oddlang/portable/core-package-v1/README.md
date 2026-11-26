# Source-free Marmish core package v1

The manifest combines the retention and configuration errors, ordered relay
normalization and bounded JSON probe envelope into a Rust 2021 library.
The graphs and Rust bindings are the package inputs. The locked registry
dependencies supply URL parsing, Serde JSON and Base64. The manifest pins
each graph and binding by byte digest, along with a registry-only Cargo.lock.
[Package manifest](../core-package.json) · [Retention and errors](../core-config-v1/domain.ttl) · [Relay policy](../core-relays-v1/graph.json) · [Probe envelope](../core-frame-v1/domain.ttl)

Use a build of Oddlang at the framework revision recorded with the native
qualification. Generate into a new output directory and compare the generated
lock to this release before building with Rust 1.97.1.

```sh
oddlang build native-package rust oddlang/portable/core-package.json /tmp/generated-marmish-core
cmp /tmp/generated-marmish-core/package.lock oddlang/portable/core-package-v1/package.lock
cargo +1.97.1 build --manifest-path /tmp/generated-marmish-core/Cargo.toml --release --locked --offline
```

The receipt applies to the original source at
`af54339d250290490d62d4475a77539b7e739071`. It covers the public Rust value API,
unchanged upstream tests, native differential observations and explicit
negative controls. [Qualification and replay](../../../verification/whole/core-package/README.md)
