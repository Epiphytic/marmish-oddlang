# Copy Manifest

The [package](../stream-copy-manifest-package.json) extends the copy Launch and helper context package with a typed binary Manifest and its Mtime value. The portable graph specifies field order, path bounds, boolean and option tags, signed seconds, nanos, permission bits, and ordered diagnostics. The namespace hashes client, server, transfer and direction independently of validation.

The original exposes the canonical encoded bytes through `Manifest::encode`. Installed verification hashes those bytes with SHA-256. The differential witness compares both the bytes and that digest. [Verification](../../../verification/whole/copy-manifest/README.md)
