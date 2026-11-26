# Release-version text grammar

The [portable graph](../../../oddlang/portable/release-version-grammar-v1/graph.json) defines the pure release-version predicate. It selects the `0.2.` and `0.3.` families, consumes a canonical unsigned 64-bit decimal and permits a delimited `-rc.` part with a nonzero unsigned 64-bit decimal.

The [selection](../../../oddlang/portable/release-version-grammar-v1/selection.json) supplies the graph IRI and entry name and pins the byte-exact `version` and `decimal` functions selected from `native_manifest.rs`. The compiler recovers the prefixes, delimiter, decimal width, nonzero flags and complete consumption from the checked Rust function shapes. The function selection is authored and its [source origin](../../../oddlang/portable/release-version-grammar-v1/origin.json) pins the original module digest and exact byte span. The original enclosing module’s method and name resolution still requires native qualification.

The verifier runs the unchanged upstream Rust `strict_vectors` and `shared_release_version_vectors` tests in the pinned package. It separately compiles the selected predicates in an isolated native Rust harness and tests their results against the upstream JSON vectors and independent boundary cases. A source mutation changes a family, delimiter, width and suffix minimum. Its dedicated decimal helper keeps the width control local to the version predicate. Both the mutated reference and projection must follow the changed boundary. The baseline and mutation also run the same cases through the predicate in its enclosing native module. A temporary archive of the original crate carries only the test harness and the declared source control. The sealed projection stages only graphs and denies reads of the original and changed Rust sources.

Use clean checkouts of Marmish at `af54339d250290490d62d4475a77539b7e739071` and Oddlang at `d322eca7fdfa2e5e257512ea477a09bb2022e9fa`.

```sh
python verification/whole/release-version-grammar/verify.py \
  /path/to/original /path/to/framework /path/to/compiler-target \
  /path/to/reference-target /tmp/release-version-grammar/receipt.json
```
