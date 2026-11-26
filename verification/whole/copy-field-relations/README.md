# Verify copy field relations

Check out the original and framework at the revisions in the [macOS ARM64 receipt](reference-aarch64-apple-darwin.json). From the companion root with Rust 1.97.1 and cached registry dependencies, run

```sh
python3 -B verification/whole/copy-field-relations/verify.py /path/to/original /path/to/oddlang /tmp/copy-field-relations-receipt.json
```

The native caller checks the exact method signatures in both libraries. The independent oracle compares all ten Manifest correspondence fields, ignored attempt and payload fields, both negotiated codecs and four identity mismatches with competing invalid Launch inputs. It checks all three build operands and validation precedence for invalid context. A repinned mutation moves the fallible digest before the identity checks and changes the reported error. The offline build and generator run with original and companion source reads denied on macOS.

The [Linux ARM64 receipt](reference-aarch64-unknown-linux-gnu.json) records the same package lock, caller and observations as macOS, including the repinned mutation. The Linux run used the offline dependency cache and the source-denied bubblewrap build.
