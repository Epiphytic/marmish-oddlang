# Launch digest and namespace

The [native package manifest](../stream-launch-hash-package.json) extends the [Launch data and validation contract](../copy-launch-v1/README.md) with two observable methods. It reuses the pinned enum, result and codec components. The extended wire graph and Rust binding are independently pinned in the package. The previous package and its lock remain reproducible.

`Launch.digest(codec)` runs the normal Launch guards in order. On success it returns SHA-256 of the domain prefix and declared ordered binary slots, followed by the negotiated codec byte. The graph assigns the byte values and widths, including the big-endian length before each raw path.

`Launch.namespace()` returns a fixed prefix followed by the lowercase SHA-256 digest of its own domain and the ordered client, server, transfer and direction fields. It is defined even for an invalid Launch and does not establish authorization.

The Rust hints bind record methods, enum names and the SHA-256 library provider. The [verification and receipt](../../../verification/whole/copy-launch-hash/README.md) compare both operations with the pinned original. The [extended helper context package](../copy-helper-context-v1/README.md) adds the checked binary context. Manifest matching, manifest and checkpoint operations, and tagged copy frames require separate qualification.
