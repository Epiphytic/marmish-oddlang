# Core configuration value contracts

This graph defines Marmish's retention value. Missing configuration uses a one-day default. An explicitly disabled value remains absent. A present value must fall between one second and 365 days, inclusive. The graph names the range failure and the public configuration error messages. The Rust binding maps those identities to exported Rust types, variants, methods and the associated maximum. Changes to a library choice require no graph change.

Oddlang recovered the interval, default and error displays by compiling a typed observer against the original public API at commit `af54339d250290490d62d4475a77539b7e739071`. The selection supplies the API names, integer width, error identities and single-interval hypothesis. It supplies no bounds, default value or error text. The 64-bit numeric coverage is sampled. The unchanged upstream test and an independent caller qualify the observed behavior.

To repeat extraction, run `oddlang recover value-domain /path/to/marmish/Cargo.toml selection.json /tmp/observed-config` and compare its graph and observation receipt with this directory. The generated Rust module implements Retention and ConfigError. RelaySet and the remaining core types have separate obligations.

From this directory, build from the graph and binding. The TypeScript build uses only the graph.

```sh
oddlang build value-domain rust . rust.json /tmp/retention-rust
oddlang build value-domain typescript . /tmp/retention-node
cmp rust.lock /tmp/retention-rust/projection.lock
cmp typescript.lock /tmp/retention-node/projection.lock
node --experimental-strip-types ../../../verification/whole/core-node/check.mjs /tmp/retention-node
```

The lockfiles bind the canonical semantic input, obligations and emitted source bytes. No original Rust file is a generation input.
