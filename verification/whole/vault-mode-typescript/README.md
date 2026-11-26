# TypeScript qualification for the published finite mode choice

The TypeScript projection stages the exact published portable graph and a
TypeScript name binding as its full input closure. The published native
caller receipts for Darwin and Linux ARM64 supply the independently
observed ordinals and absence of native equality traits.

Node observes `Interactive = 0` and `Verify = 1`. Only valid ordinals pass
the checked conversion. TypeScript rejects unchecked integers and both
portable equality methods on this choice. The generated capability is
`none`. JavaScript strict equality remains available on the numeric values:
a value equals itself, and the two different ordinals compare unequal.
The receipt records that host operation separately from the declared
portable capability.

The projected obligations are byte-identical to the published portable
obligations. The receipt pins the exact projection lock, generated
TypeScript source, type declarations, verifier, original graph and native
witness.

## Reproduce

Use a clean Oddlang checkout at the receipt's framework revision, with
Rust 1.97.1 and Node 22.22.2 available. TypeScript 5.9.3 is selected from the
offline npm cache. The verifier builds the compiler into the external
target and obtains graph and witness bytes from their published commit.

```sh
python3 -B verification/whole/vault-mode-typescript/verify.py \
  "$FRAMEWORK" "$COMPILER_TARGET" "$RECEIPT"
```
