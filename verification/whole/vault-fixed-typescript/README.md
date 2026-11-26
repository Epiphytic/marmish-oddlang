# Vault fixed-byte width in Node TypeScript

The published [32-byte graph](../../../oddlang/portable/vault-fixed-bytes-v1/graph.json) projects into a native Node TypeScript module. The verifier stages that single graph, checks that the sandbox denies reads of the Rust binding and Rust witness, and projects the module with network access denied.

Node exercises exact-width admission and the declared value and failure identities while checking that writes to the input and to a returned copy have no effect on the stored value. The holdout includes Node buffers, byte arrays from another realm, detached arrays and shadowed length properties. Reordering the graph reproduces the exact artifacts. A graph mutation to 31 bytes shifts the native width boundary.

The obligation catalog records deterministic destruction and storage erasure as unsupported for this target. The replay uses local byte literals.

Use a clean Oddlang checkout at `0f8410636148e3afa96005c9c24390fbd32eb602` and Node.js with native TypeScript support.

```sh
python verification/whole/vault-fixed-typescript/verify.py \
  /path/to/framework /path/to/compiler-target /tmp/vault-typescript/receipt.json
```
