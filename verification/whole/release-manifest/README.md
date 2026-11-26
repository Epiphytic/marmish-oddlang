# Original release manifest parser observations

This corpus exercises the pinned `marmish-native` release manifest parser at
`af54339d250290490d62d4475a77539b7e739071`. It covers
`Manifest::parse`, `id`, and the asset-name formatter applied to the four
parsed file names. Each successful row records the complete native value,
including the ordered format numbers and file rows, the decoded digest in
hex, the exact unsigned byte count and the numeric mode.

The authored corpus tests all target, signing, notarization and minimum
family combinations in its finite cross-product. It also tests numeric
spelling and widths, minimum-version boundaries, exact per-asset maxima,
the aggregate boundary, ordered line and token requirements, and malformed
byte framing. The 4095-, 4096- and 4097-byte inputs all violate the record
grammar. Those samples therefore observe rejection at those lengths.
The source itself specifies a 4096-byte outer ceiling.

The parse path checks byte framing and record layout before version, target
and commit, then checks signing and platform constraints, the format vector,
file rows and the aggregate size. All rejected inputs in scope expose
`Invalid("install_manifest")`, so the failing field and its precedence
are source observations only. Raising the license ceiling by one byte
admits the corresponding below-aggregate case. The aggregate-over case
still rejects under that mutation.

Run the oracle from a pinned original checkout with Rust 1.97.1 and the
offline Cargo dependencies available.

```sh
python3 -B verification/whole/release-manifest/verify.py /path/to/original \
  /tmp/release-manifest-observation.json --target-dir /path/to/cargo-cache
```

The verifier archives the exact original revision into a temporary tree,
compiles the original parser, and runs the untouched `strict_vectors` and
`shared_release_version_vectors` tests. The added test caller only invokes
the three pure methods. It then runs the identical corpus against the
single changed source expression in the temporary tree. The source pins,
input digest, authored oracle and native results are independently
checkable. Installation, path and directory discovery, and asset I/O
require separate qualification. This is original-source evidence for a
later parser projection.

The archived [ARM64 macOS observation](aarch64-apple-darwin.json) covers 224
cases, including 34 acceptances, and both unchanged pure parser tests. The
license-cap mutation changes exactly one case, `size-3-16777217`.
