# Public API convergence

The [workspace manifest](../../../oddlang/portable/workspace-v7-converged-public-api.json) combines the recovered public prototype constant with the engine error variants, typed reasons and native identity choice. The combined release uses the existing Cargo lock. Its seven member rows each reproduce the complete corresponding parent row, including its generated file digests. Run `python3 -B compose.py ../../../oddlang/portable --check` from this directory.

The [Darwin import audit](darwin-imports.json) records results from compiling independent consumers against all seven generated release libraries. The [candidate profile](import-profile.json) identifies the exact companion inputs, framework commit and workspace lock. Use the existing seven-public-imports runner, adding `--candidate-profile` with this profile. All other options are as described in the [audit guide](../seven-public-imports/README.md).

| Darwin ARM64 | Baseline | Combined |
| --- | ---: | ---: |
| Original type/module/alias names resolved | 65 | 70 |
| Original names unresolved | 75 | 70 |

Newly importable names are the engine's `NotExecutableReason`, `NotFoundReason`, and `ResolutionReason`, plus native's `identity` and `identity::BackendChoice`. Every previously resolved name remains importable. A separate value probe compiled and read `marmish_prototype::MAX_LINE` as 131072. The prototype's `serve` function signature remains unresolved. These observations identify the downstream Rust API surface on this build. They do not establish CLI or network interoperability.

The [Darwin ARM64](aarch64-apple-darwin.json) and [Linux ARM64](aarch64-unknown-linux-gnu.json) sealed builds replay the same contracts at verifier revision `3a86cf5f9c29c076886f21475cf8e04cc60b05ca`. Both denied all 17 source and caller probes, built the seven-member release from identical inputs and preserved the two audited parent locks. Both hosts exercised 26 distinct engine error variants, 38 rejected engine type usages, 12 rejected identity type usages, all four private reader cases and the public constant. Their positive engine and identity output and their expected compiler diagnostic codes agree across hosts. The import inventory above was measured on Darwin ARM64.
