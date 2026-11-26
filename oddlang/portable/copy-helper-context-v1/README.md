# Copy helper context

The [package manifest](../stream-copy-helper-context-package.json) adds an independently pinned binary layout and native binding to the existing Launch package. The layout declares the domain bytes, big-endian integers, fixed byte fields, length-prefixed group and build atoms, direction tags and the literal helper entrypoint. The direction tag table comes from the pinned copy enum contract.

The generated `HelperContext` exposes `validate`, `encode` and `decode`. Construction validation and structural decoding declare separate named failures. The decoder checks input size and the domain first, reads fields in order, checks the entrypoint and trailing bytes, then validates the constructed record. The 4096-byte defensive encode limit is unreachable for a valid HelperContext because its maximum field lengths imply a smaller encoding.

These methods cover the wire representation and its value checks. The context does not grant authority. Start binding, build identity comparison and installed helper execution have separate contracts and require separate qualification. [Independent verification](../../../verification/whole/copy-helper-context/README.md)
