# Marmish workspace data shapes v1

This archive records candidate local data shapes from the original Marmish
release reference, `af54339d250290490d62d4475a77539b7e739071`. Oddlang
`5ec32f8177cf9d7b4ffcce0c192c5553af7f2042` extracted each graph using
Rust 1.97.1 in Cargo release mode. The archive contains typed products, sums,
unions, ordered members, nominal type references and owner-scoped gaps.
The compiler adapter emits no implementation bodies, source excerpts,
source coordinates, compiler intermediate representation or compiled objects.

[release.json](release.json) enumerates the workspace's seven Cargo packages
and their 17 library and binary targets using the upstream Cargo metadata.
Each profile identifies its package, target kind and name, target triple,
default features and explicit features. The matrix includes each target in
the package's default profile where eligible. It also includes the engine
targets with `fault-injection` and the native targets with `installed-checks`.
Cargo's required features explain the feature-only binaries. This matrix uses
single-package Cargo selections. Workspace-wide feature unification,
downstream feature choices and tests are outside these profile observations.

Every candidate cell has its own compiler lock under `locks/`. Equal graph
bytes share one content-addressed object under `graphs/`, including across
feature profiles and empty local data surfaces. The profile lock retains the
target identity in those cases. Scope a local declaration ID to its cell.
External nominal references still carry package-instance gaps where the
adapter has no exact package binding. The graph's shape coverage and gap
counts apply to the selected target, and a zero declaration count means the
adapter found zero local data declarations for that build.

A candidate records an observation, with its completeness limited by the
listed gaps. These include array lengths, generic constraints, explicit
discriminants, compiler-generated identity, native layout, public visibility
and external package instances. The archive leaves behavior, signatures,
cross-package identity, layout equivalence, and source-independent contract
qualification open. Each `unobserved` row identifies a profile with no
archived graph. All 24 applicable macOS ARM64 profiles have archived graphs.

## Replay and checks

Use clean checkouts at the revisions above. Install the pinned Rust toolchain
and `rustc-dev`, supply the selected host's native build dependencies and
populate Cargo's cache before running the locked, offline commands. Each
recovery builds Oddlang and invokes its generic adapter with fresh compilation
directories. An interrupted run can reuse its work directory and validated
cells. Use a new work directory for independent replay.

```sh
runner=verification/whole/shapes/workspace_shapes.py
python3 -B "$runner" /path/to/original /path/to/oddlang verify
python3 -B "$runner" /path/to/original /path/to/oddlang recover /tmp/shapes-repeat \
  aarch64-apple-darwin --target marmish_core --feature-mode default
python3 -B "$runner" /path/to/original /path/to/oddlang compare /tmp/shapes-repeat
python3 -B verification/whole/shapes/test_archive.py /path/to/original /path/to/oddlang \
  oddlang/portable/workspace-shapes-v1
```

Omit `--target` and `--feature-mode` to recover the whole selected host matrix.
Pass `--target` more than once to select several targets. To seal a fresh
archive, use `seal NEW_ARCHIVE WORK_DIR [WORK_DIR ...]` with the same runner.
The verifier compares the inventory against Cargo metadata, checks the complete
profile matrix, validates payload digests and locks, and checks the graph's
closed structure and order of declarations, parameters, arguments, variants,
fields, tuple elements and gaps. The comparison checks raw graph bytes by
digest and checks the semantic lock digest. That digest binds the source
revision from the release and excludes host Cargo configuration. The raw lock
preserves both fields as emitted. A different local Cargo configuration may
still change the graph and cause replay to fail.

The macOS core and stream default profiles, fault-only frontend, and
installed-only check reproduced both graph and semantic lock digests in
fresh compilation directories. The two feature-gated replays also used a
separate source checkout. A native Linux ARM64 cache control reproduced the
default engine worker's raw graph and lock bytes with and without a native C
compiler cache. Rust target directories remained fresh in those runs.

The default engine library illustrates target-specific coverage. The macOS
graph has 216 declarations and the Linux ARM64 graph has 209. Seven IDs
occur only on macOS, including `ZombieIdentity` and `DarwinStage`. Five
shared IDs have differing shapes. The release preserves each observation
and its gaps under the corresponding triple.
