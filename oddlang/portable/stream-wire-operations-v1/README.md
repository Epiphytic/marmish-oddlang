# Stream wire operation contract

This contract selects the canonical JSON admission API observed in the pinned Marmish wire module. The shapes and wire adapters are pinned in [stream-wire-schema-v1](../stream-wire-schema-v1/README.md). The graph adds envelope field mappings, size and literal checks, ordered request and response rules, request clock admission, and the two SHA-256 identities. The Rust overlay binds the public names and error constructors. The separate shared declaration graph models the three public case enums.

The graph depends on the shared error contract, dictionary-free bounded integrity block, and Copy Launch contract. OpenCopy calls the Launch validation role before comparing account bindings. CopyHello admits the Copy contract's version 1. The wire layer leaves authentication, replay state, and transfer execution to its callers.

The standalone package uses [stream-wire-package.json](../stream-wire-package.json) to compose these projections with the pinned dependencies. The independent native replay builds both peers and runs the same caller against each. [Verification](../../../verification/whole/stream-wire-operations/README.md)
