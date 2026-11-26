# Copy field relations

The [package](../stream-copy-field-relations-package.json) extends the Manifest package with three methods. `Launch::matches_manifest` compares ten explicitly typed fields. The launch attempt and the manifest-only payload metadata are independent of that comparison. `HelperContext::validate_start` validates the context and compares identities and direction before invoking the checked Launch digest. `HelperContext::validate_build` validates the context and compares the expected version, target and commit.

The graph owns the operand pairs, short-circuit order and named mismatch errors. Native field paths, checked method names and the typed error constructors are Rust hints. These checks establish consistency of supplied values. The authenticated peer and installed helper runtime have separate obligations. [Verification](../../../verification/whole/copy-field-relations/README.md)
