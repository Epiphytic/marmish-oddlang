# Two-member native workspace qualification

The verifier requires clean checkouts of both Git revisions in the receipt. It builds the pinned framework, checks every manifest and component input, then assembles and compiles the native workspace with upstream files and network access unavailable. After that clean release build succeeds, it injects the original core integration test file and the byte-exact upstream codec and window test modules at native privacy scope. The receipt records all seventeen test names and the test hashes.

```sh
python3 -B verification/whole/workspace/verify.py \
  /path/to/pinned/oddlang /path/to/pinned/marmish /tmp/native-workspace-receipt.json
cmp /tmp/native-workspace-receipt.json verification/whole/workspace/reference-aarch64-apple-darwin.json
```

The native macOS ARM64 and Linux ARM64 receipts have identical workspace, test module and test-name observations. Linux uses a separate network namespace, an allowlist of files exposed with bubblewrap, and a process with zero effective capabilities. Its replay command uses the same inputs as the macOS run. Compare its output with `reference-aarch64-unknown-linux-gnu.json`. Individual member releases also carry independent public caller and mutation evidence.
