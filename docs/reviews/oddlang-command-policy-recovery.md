# Command policy recovery review

Claude Code 2.1.332 reviewed the source extractor, foreground projection, portable schema, decision binding and differential fixtures against Marmish commit `af54339d250290490d62d4475a77539b7e739071` using read-only tools in auto permission mode. It approved configuration parsing, bounds, principal layers, byte glob matching and named-executable decisions with no blocking findings.

The reviewer identified implicit support for positional arrays in Serde. Recovery now records the three nested records' positional field order and rejects source-order drift. The differential suite includes positional inputs, version boundaries and match-budget cases with an alias. The decision binding is a reviewed transcription pinned to its source. Full daemon command handling is outside the review. [Verification suite](../../verification/whole/command-policy)

A second read-only review approved the positional contract and its source-order and graph-order regression tests. Positional conformance covers the three nested policy records in an object-form foreground configuration.
