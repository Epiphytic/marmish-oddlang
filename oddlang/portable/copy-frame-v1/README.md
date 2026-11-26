# Copy Frame and incremental decoder

The [native package](../stream-copy-frame-package.json) includes the complete public `marmish_stream::copy_protocol` library surface for the pinned release. It adds all 11 binary tagged payloads, the bounded incremental decoder and protocol constants on top of the checked copy records and relations.

The frame graph owns variant shapes, exact discriminants, byte and offset bounds, per-direction validation order, literal disposition and owner tags, and poison errors. The Start and Manifest cases reuse prefix-only typed record codecs. The package retains complete-record Manifest validation order separately from Manifest as a frame payload. [Verification and structural reconciliation](../../../verification/whole/copy-frame/README.md)
