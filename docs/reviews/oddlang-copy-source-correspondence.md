# Source-derived copy correspondence review

Claude CLI in auto mode followed the released relations through recovery and native verification. It confirmed that the mutation comes from an archive of the pinned upstream revision and that both independently compiled upstream revisions and both generated packages are exercised by an independent oracle. It requested that verification build the CLI from the recorded framework revision, check the released package lock and record the caller hash.

The verifier builds the CLI from an archive of the recorded framework revision, checks the reproduced package lock byte for byte, and records both the caller hash and both generated relation hashes. Native cases on macOS ARM64 were repeated after these changes with both reference source trees denied during library generation and compilation.
