# Copy Checkpoint

The [package](../stream-copy-checkpoint-package.json) extends the prior copy shapes with the checked binary Checkpoint and its FileIdentity values. The graph specifies the version, checked nested Manifest, Phase tags, signed timestamps, identity fields, offset and phase constraints, bounded path and SHA-256 footer. It declares the footer's domain and distinct errors for the aggregate bound, checksum, version, nested record, interpreting fields and final validation.

The receiver's filesystem transactions and authenticated admission are assessed separately from this record. [Verification](../../../verification/whole/copy-checkpoint/README.md)
