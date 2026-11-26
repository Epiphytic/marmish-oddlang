# Release-version text grammar

The [portable graph](../../../oddlang/portable/release-version-grammar-v1/graph.json) defines the pure release-version predicate. It selects the `0.2.` and `0.3.` families, consumes a canonical unsigned 64-bit decimal and permits a delimited `-rc.` part with a nonzero unsigned 64-bit decimal.

The [selection](../../../oddlang/portable/release-version-grammar-v1/selection.json) supplies the graph IRI and selects `version` from pinned `native_manifest.rs`. The compiler recovers the prefixes, delimiter, decimal width, nonzero flags and complete consumption from the checked Rust function shapes. The source module's surrounding Rust scope still requires native qualification.

The verifier runs the unchanged upstream Rust `strict_vectors` test in the pinned package. It compiles the selected predicates and tests their results against the upstream JSON vectors and independent boundary cases. A source mutation changes a family, delimiter, width and suffix minimum. Both the mutated reference and projection must follow the changed boundary. The sealed projection stages only graphs and denies reads of the original and changed Rust sources.

Use clean checkouts of Marmish at `af54339d250290490d62d4475a77539b7e739071` and Oddlang at `c78a9b82ac0aa6148054a5c36eb519689223ef9d`.

```sh
python verification/whole/release-version-grammar/verify.py \
  /path/to/original /path/to/framework /path/to/compiler-target \
  /path/to/reference-target /tmp/release-version-grammar/receipt.json
```
