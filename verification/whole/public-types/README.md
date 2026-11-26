# Public type spellings and field reading

The qualified boundary is a locked library's public type namespace. Each local
module is visited at its shortest lexicographically first spelling. The
observers check all direct edges at that path, including module aliases and
cycle edges. They also record fields and their audiences. External modules
are treated as resolutions at the boundary.

The original is Marmish `af54339d250290490d62d4475a77539b7e739071`.
The generated input is the source-isolated v5 workspace built with Oddlang
`fa40d7fab0bd6308ed7b13ef2aa97206145ebd36`. Its workspace lock SHA-256 is
`12f45ddf99163d5a4ea64361e38289c7cbd00f3717b6e5ae1899e4c87d5470fd`.
The public-type extractor uses Rust 1.97.1. The archived comparisons use
Darwin ARM64 with default features in release profiles.

Build the exact extractor cited in `provenance.json` with
`cargo +1.97.1 build --locked --offline`. Run its
`verification/rust-recovery/probe_exports.py ODDLANG MANIFEST PACKAGE OUTPUT
--default-features` on core and stream in both workspaces and on engine in
the original. Each run repeats compiler extraction and compares the graph and
lock bytes, builds rustdoc JSON independently, and compiles a separate Rust
caller. The inspector can disclose individual discrepancies and field gaps
without including the docs' source text or a compiler's object files.
Run the same framework directory's `compare_exports.py` with the two
`exports.json` files to reproduce each paired comparison.

The comparison records spellings, kind, public field reading, restricted
field names and within-crate alias equivalence. The generated window types have
different restricted storage fields. The public spelling sets and tested
public field reads agree. Visibility does not establish constructors, pattern
compatibility, associated items, macros, implementation behavior, or full
type equality. Non-default real release profiles and external-module
descendants are outside this evidence. Generic receivers that the automatic
field probe cannot instantiate are recorded in the per-profile evidence.

The original engine adds 48 type names from its release profile. All agree
with the rustdoc observer, and its separate caller checks 84 public field
reads and 33 restricted field rejections. The engine row is an observation of
upstream and has no generated comparison in this release.

The legacy compatibility receipt compares raw compiler structure and shape
observations across the base and new adapters for the same original stream
profile. The graph bytes agree. The adapter hashes in new locks change
because the adapter inputs changed. Existing legacy locks remain historical
receipts for their pinned inputs.

The unrelated fixture retains both the primary and alternative identities
for its ambiguous glob. Rustdoc lists the same name, while the table keeps it
outside the ordinary public edges. The framework's fixture suite also checks
the denied and allowed glob lint outcomes and the separately resolved
visibility-lint case.
