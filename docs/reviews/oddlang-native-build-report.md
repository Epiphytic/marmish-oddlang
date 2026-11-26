# Native release-info review

Claude approved compiler revision `799a2467b0eccd9e5d902c5b6a81b1e289536b50` and companion source `dec0e54d22ac9bde223ac25794b571b01a95c606`. Framework review checked reuse of the existing guard expressions, operation identity, ordered escaping, numeric widths and target-script sealing. The unrelated fixture checks all four guard types, operation mismatch, malformed declarations, changeable build metadata, and exact output.

Companion review checked the version and target against native build facts, the represented original revision against the pinned release, and all five authored format values and original declaration widths. The v7 gate seals the build script, native artifacts and independently accepted source and compiler revisions. The original client and daemon executables serve as the reference for each CLI case. The v6 workspace was regenerated with its prior lock and output inventory.

Generated profile, journal and configuration version facts agree with the report. Engine and command formats are represented upstream values. Compiler extraction of the original format references, worker and installed-tool metadata, startup descriptor reporting, broken stdout behavior and additional platforms remain separate.

[Framework PR](https://github.com/Epiphytic/oddlang/pull/72) · [Workspace qualification](../../verification/whole/workspace/README.md#workspace-v7)
