# Pinned Marmish acceptance inventory

This inventory sets the baseline for the whole-project recovery against Marmish commit `af54339d250290490d62d4475a77539b7e739071`. Run the checks with Git, the pinned Rust toolchain (1.97.1) and Node. The local reference clone needs both the prior and current Git objects. The checker uses a disposable clone and offline Cargo metadata. Reference sources are verification inputs only.

```sh
cargo +1.97.1 run --offline --locked --manifest-path verification/whole/canonical-public-data/Cargo.toml \
  --bin release-inventory -- check /path/to/reference/clone
```

[release.json](release.json) defines reviewed platform minima, profiles, surface owners and checks, external program provenance, and the split distribution layout. [inventory.json](inventory.json) derives two lock digests, target metadata and paths, feature gates, file families and release names from the pin. The seven workspace packages declare 29 Cargo targets. The standalone installed fixture adds another package and target. The 214 supporting files include 54 Python test modules and 21 Node test modules. Rust harness and doctest applicability come directly from Cargo metadata. The inventory spans three deployment targets and three feature selections. A platform cell records an obligation to run.

[inventory.ttl](inventory.ttl) lets a consumer query packaging, ownership, target kinds, required features, programs and acceptance obligations without loading Rust code. For example, this query returns feature-gated target names.

```sparql
PREFIX i: <urn:oddlang:inventory:>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?name ?feature WHERE {
  ?target a i:CargoTarget ; rdfs:label ?name ; i:requires [ rdfs:label ?feature ] .
} ORDER BY ?name
```

The checker regenerates both projections, compares all 15 asset names with the release schema from the pinned original, and checks that removing the standalone workspace fails. The original documents 162 passing installed transfer cases from an earlier candidate (`b666ae6a`) in [its release report](https://github.com/Epiphytic/marmish/blob/af54339d250290490d62d4475a77539b7e739071/docs/validation/v0.3.1-release.md). Running the upstream suite on generated binaries, operation-by-operation equivalence, and reference runs for each profile and host require separate runtime receipts. The file-group owner for general support tools is provisional pending per-tool behavior mapping. The legacy build recipe pins and builds `wnd`. An independently reproduced `wn` fixture remains open.

## Source-free build target coverage

The seven upstream workspace packages declare seven library targets and ten binary targets. With default features, seven of those binaries are eligible. The other three require explicit features. The default Oddlang workspace and the separate MDK test workspace both declared and built all seven library target names and two binary target names on macOS ARM64 and Linux ARM64.

| Upstream binary targets | Declared and built in either workspace | Absent from both workspace manifests |
| --- | --- | --- |
| Default features | `marmish`, `marmishd` | `marmish-engine-worker`, `marmish-engine-bridge`, `marmish-keychain`, `marmish-release-tool`, `marmish-worker` |
| `fault-injection` | — | `marmish-test-frontend` |
| `installed-checks` | — | `marmish-installed-check`, `marmish-installed-fixture` |

From the companion root, run the inventory comparison for a selected native receipt:

```sh
cargo run --quiet --locked --offline --manifest-path verification/whole/suite/Cargo.toml --bin candidate-targets -- \
  verification/whole/suite/canonical-aarch64-apple-darwin.json
```

The report checks the upstream revision, workspace and member pins, suite evidence, and the package that declares each target. It measures artifact presence. Behavioral and API compatibility have separate gates. The standalone installed fixture, examples, integration tests and release assets are outside this target count. [Default workspace receipts](../../whole/suite/README.md#combined-offline-workspace) · [MDK workspace receipts](../../whole/suite/README.md)
