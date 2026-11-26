# Seven-member public import audit

The ARM64 macOS audit compiled downstream imports for the 140 distinct public type, alias and module spellings in the archived upstream inventories. It used all seven libraries from the unmodified release build, with the generated workspace and member files checked against the pinned workspace lock.

| Library | Imports compiled | Imports missing |
| --- | ---: | ---: |
| Core | 6 | 0 |
| Engine | 2 | 46 |
| MDK | 2 | 3 |
| Native | 13 | 23 |
| Prototype | 0 | 0 |
| Stream | 35 | 0 |
| Vault | 7 | 3 |
| **Total** | **65** | **75** |

Every missing spelling produced rustc `E0432`. The [receipt](darwin-arm64.json) records exact diagnostics and successful imports by spelling. It measures importability. Type identity, signatures, behavior and downstream source compatibility are separate criteria.

The prototype inventory includes zero type, alias and module spellings. A separate compiled constant probe reads the original `MAX_LINE` as a `usize` with value 131072. A compile-only probe checks that the original `serve` can be coerced to `fn(R, W) -> std::io::Result<()>` for `R: Read` and `W: Write`. The probe takes a function item and never invokes it. Both generated symbols are absent and produce `E0425`. These probes sit outside the table.

## Replay

Use the original checkout at `af54339d250290490d62d4475a77539b7e739071` and the generated workspace identified by input revision `9d364408d2b26f65defd5e88bf6727960250d789`, compiler runtime `59f947dafdb4cf3055e35216857472ada895a76a`, and workspace-lock SHA-256 `fdfa5b16d9172fe97705ee5794251c6168d0e226ea159334a52286e2e7172b68`. Run with Rust 1.97.1 and offline dependency caches. Both Cargo target directories are outside the source trees.

```sh
python3 verification/whole/seven-public-imports/verify.py \
    /path/to/generated-workspace /path/to/candidate-target \
    /path/to/original /path/to/original-target /tmp/seven-public-imports.json
```

The verifier checks the generated tree before and after the probes. Supply `--release-inputs` if the pinned workspace manifest and lock are in a separate companion checkout. With `--candidate-artifacts` and `--original-artifacts`, it consumes Cargo JSON from completed builds and the corresponding local artifacts. Otherwise it runs the locked offline release builds. Absolute paths and compiler build outputs can affect artifact digests in separate runs.
