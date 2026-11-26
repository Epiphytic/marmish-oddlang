# Converged CLI, diagnostics and public API

This seven-member workspace composes the independently qualified public
API, offline CLI and public diagnostic releases. The workspace keeps the
shared Cargo dependency lock and all seven package roles. The public API
parent's six other member locks are preserved. Within the shared native
member, the CLI grammar, identity choice and diagnostic state program
retain their reviewed graph and binding pins.

The composition uses the common seven-member manifest as the base. For the
same package, a component is keyed by its kind and module. Inherited
components keep their order, new components are appended in key order,
and identical edits can occur in several parents. Conflicting component
edits, duplicate keys, missing inherited components and changes to shared
package metadata or sibling edges fail before generation.

The Darwin ARM64 verifier builds the libraries and both release binaries
together from the staged graph closure under a network-denying sandbox.
The original, framework and companion checkouts are inaccessible during
generation and compilation. The public import check compiles independent
callers against actual Cargo artifacts. It reuses the earlier Darwin
type inventory and keeps the prototype value and compile-only signature
probes separate from the import count. The native diagnostic, engine error
and identity probes link against the same candidate build.

The CLI probe compares both roles with the exact archived help, version
and usage receipts. The invocation set supplies no remote destination or
command. Its sandbox denies writes, network and forks. The four previously
uncovered leading-option cases retain the same observed differences.

## Reproduce

Build Oddlang at the receipt's framework revision with Rust 1.97.1 and
`cargo build --release --locked --offline`. Use the receipt's workspace
input revision, the pinned original checkout and matching Darwin ARM64
original binary directory. Keep the compiler target and product target
outside the original and generated checkouts.

```sh
lane=verification/whole/converged-diagnostics-cli
python3 -B "$lane/test_compose.py"
python3 -B "$lane/compose.py" --check
python3 -B "$lane/verify.py" "$INPUT_REVISION" "$ODDLANG" "$FRAMEWORK" \
  "$ORIGINAL" "$ORIGINAL_BIN" "$ORIGINAL_TARGET" "$PRODUCT" "$TARGET" "$RECEIPT"
```

The generated product directory must be absent. Library and binary hashes
identify the artifacts used by each individual run. Independent builds can
produce different artifact hashes while reproducing the same sealed
workspace, diagnostic bytes, CLI rows and import outcomes.
