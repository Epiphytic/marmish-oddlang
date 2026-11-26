# Copy helper context on macOS ARM64

The original library, generated library and independent byte oracle agree on 108 native constructions and 2,159 binary decode cases. The verifier compares exact encoded data, complete decoded records and named validation and parse failures. The corpus includes every truncation point of both a normal record and a valid 1,404-byte record, every direction byte, length prefixes, aggregate limits, and combined invalid cases. One repinned field-order mutation produced the exact independently predicted changed bytes.

See the [reproduction procedure](../../verification/whole/copy-helper-context/README.md) and [host receipt](../../verification/whole/copy-helper-context/reference-aarch64-apple-darwin.json). The result covers `validate`, `encode` and `decode` on the public library type. Cross-record start binding, build identity comparison, and installed copy helper behavior require separate qualification.
